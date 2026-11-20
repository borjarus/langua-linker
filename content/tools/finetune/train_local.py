#!/usr/bin/env python3
"""
Local fine-tuning of Bielik 1.5B on Apple Silicon GPU (Metal/MPS) for LanguaLinker on-device associations (ADR-23).
Replaces Google Colab with local training on Apple M3 Max (128 GB Unified Memory).
"""

import os
import sys
import json
import re
import random
import hashlib
import datetime
import subprocess
from pathlib import Path
from collections import Counter

# Configuration matching ADR-23 and bielik_associations.ipynb
MODEL_ID = 'speakleash/Bielik-1.5B-v3.0-Instruct'
RUN_NAME = 'bielik-1.5b-associations-v1'
QUANT = 'Q4_K_M'
RELEASE_TAG = 'model-bielik-associations-v1'
REPO = 'borjarus/langua-linker'

EPOCHS = 2
LEARNING_RATE = 2e-4
LORA_RANK = 16
MAX_LENGTH = 1024
BATCH_SIZE = 8
GRAD_ACCUM = 2
EVAL_SAMPLES = 120

REPO_ROOT = Path(__file__).resolve().parents[3]
TARGET_DIR = REPO_ROOT / 'target'
DATASET_DIR = TARGET_DIR / 'llm-dataset'
TRAIN_FILE = DATASET_DIR / 'train.jsonl'
EVAL_FILE = DATASET_DIR / 'eval.jsonl'
OUTPUT_DIR = TARGET_DIR / 'finetune-output'
MERGED_DIR = TARGET_DIR / 'merged'
LLAMA_CPP_DIR = TARGET_DIR / 'llama.cpp'


def load_env_token():
    token = os.environ.get('HF_TOKEN')
    if token:
        return token.strip()
    env_file = REPO_ROOT / '.env'
    if env_file.exists():
        with open(env_file, encoding='utf-8') as f:
            for line in f:
                line = line.strip()
                if line.startswith('HF_TOKEN='):
                    return line.split('=', 1)[1].strip().strip('"\'')
    return None


def read_jsonl(path):
    with open(path, encoding='utf-8') as f:
        return [json.loads(line) for line in f if line.strip()]


def main():
    print(f"=== LanguaLinker On-Device Association Fine-Tuning ===")
    print(f"Base model: {MODEL_ID}")
    print(f"Run name:   {RUN_NAME}")
    print(f"Platform:   Apple Silicon Metal / MPS")

    hf_token = load_env_token()
    if not hf_token:
        print("\n[BŁĄD] Brak tokenu Hugging Face (HF_TOKEN)!")
        print("Model speakleash/Bielik-1.5B-v3.0-Instruct wymaga zaakceptowania licencji na Hugging Face")
        print("oraz podania tokenu z uprawnieniem 'read'.")
        print("Ustaw zmienną środowiskową HF_TOKEN lub dopisz ją do pliku .env:")
        print("  HF_TOKEN=hf_...\n")
        sys.exit(1)

    if not TRAIN_FILE.exists() or not EVAL_FILE.exists():
        print(f"\n[BŁĄD] Brak plików ze zbiorami danych w {DATASET_DIR}!")
        print("Wygeneruj je poleceniem:")
        print("  cargo run -p langualinker-application --bin llm-dataset -- content/decks target/llm-dataset")
        sys.exit(1)

    import torch
    from huggingface_hub import login
    login(token=hf_token)

    device = torch.device('mps' if torch.backends.mps.is_available() else 'cpu')
    print(f"Urządzenie PyTorch: {device}")

    train_rows = read_jsonl(TRAIN_FILE)
    eval_rows = read_jsonl(EVAL_FILE)
    print(f"Załadowano: {len(train_rows)} train / {len(eval_rows)} eval")
    print("Rozkład w zbiorze treningowym:", Counter((r['meta']['target'], r['meta']['method']) for r in train_rows))

    from transformers import AutoTokenizer, AutoModelForCausalLM
    print("\n--> Ładowanie tokenizera...")
    tokenizer = AutoTokenizer.from_pretrained(MODEL_ID, token=hf_token)
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token
    END = '<|im_end|>'

    def encode(row):
        messages = row['messages']
        prompt = tokenizer.apply_chat_template(messages[:-1], tokenize=False, add_generation_prompt=True)
        full = prompt + messages[-1]['content'] + END
        prompt_ids = tokenizer(prompt, add_special_tokens=False)['input_ids']
        ids = tokenizer(full, add_special_tokens=False)['input_ids'][:MAX_LENGTH]
        labels = ([-100] * len(prompt_ids) + ids[len(prompt_ids):])[:len(ids)]
        return {'input_ids': ids, 'labels': labels}

    print("--> Tokenizacja danych treningowych i walidacyjnych...")
    train_enc = [encode(r) for r in train_rows]
    eval_enc = [encode(r) for r in eval_rows]
    lengths = sorted(len(e['input_ids']) for e in train_enc)
    print(f"Długość tokenów: mediana {lengths[len(lengths) // 2]}, max {lengths[-1]}")
    assert all(any(l != -100 for l in e['labels']) for e in train_enc), 'Odpowiedź ucięta przez MAX_LENGTH'

    from peft import LoraConfig, get_peft_model
    from transformers import Trainer, TrainingArguments

    print("\n--> Ładowanie modelu bazowego w bfloat16...")
    model = AutoModelForCausalLM.from_pretrained(
        MODEL_ID,
        dtype=torch.bfloat16,
        token=hf_token,
    )
    model.to(device)
    model.gradient_checkpointing_enable()
    model.enable_input_require_grads()

    print("--> Konfiguracja LoRA (rank 16, target all-linear)...")
    lora_config = LoraConfig(
        r=LORA_RANK,
        lora_alpha=2 * LORA_RANK,
        lora_dropout=0.05,
        target_modules='all-linear',
        task_type='CAUSAL_LM',
    )
    model = get_peft_model(model, lora_config)
    model.print_trainable_parameters()

    def collate(batch):
        width = max(len(b['input_ids']) for b in batch)
        pad = lambda seq, value: seq + [value] * (width - len(seq))
        return {
            'input_ids': torch.tensor([pad(b['input_ids'], tokenizer.pad_token_id) for b in batch]),
            'labels': torch.tensor([pad(b['labels'], -100) for b in batch]),
            'attention_mask': torch.tensor([pad([1] * len(b['input_ids']), 0) for b in batch]),
        }

    total_steps = (len(train_enc) // (BATCH_SIZE * GRAD_ACCUM)) * EPOCHS
    warmup_steps = max(1, int(total_steps * 0.03))

    args = TrainingArguments(
        output_dir=str(OUTPUT_DIR),
        num_train_epochs=EPOCHS,
        learning_rate=LEARNING_RATE,
        per_device_train_batch_size=BATCH_SIZE,
        per_device_eval_batch_size=BATCH_SIZE,
        gradient_accumulation_steps=GRAD_ACCUM,
        lr_scheduler_type='cosine',
        warmup_steps=warmup_steps,
        logging_steps=20,
        eval_strategy='epoch',
        save_strategy='no',
        bf16=True,
        report_to='none',
        remove_unused_columns=False,
    )

    print("\n--> Rozpoczynam trening LoRA...")
    trainer = Trainer(
        model=model,
        args=args,
        train_dataset=train_enc,
        eval_dataset=eval_enc,
        data_collator=collate,
    )
    trainer.train()
    eval_metrics = trainer.evaluate()
    print("Wyniki walidacji po treningu:", eval_metrics)

    print("\n--> Ewaluacja jakościowa skojarzeń...")
    model.eval()
    random.seed(7)
    sample = random.sample(eval_rows, min(EVAL_SAMPLES, len(eval_rows)))
    end_id = tokenizer.convert_tokens_to_ids(END)

    def generate(messages, temperature=0.7):
        prompt = tokenizer.apply_chat_template(messages, tokenize=False, add_generation_prompt=True)
        inputs = tokenizer(prompt, return_tensors='pt', add_special_tokens=False).to(device)
        with torch.no_grad():
            out = model.generate(
                **inputs,
                max_new_tokens=256,
                do_sample=True,
                temperature=temperature,
                top_p=0.9,
                eos_token_id=end_id,
                pad_token_id=tokenizer.pad_token_id,
            )
        return tokenizer.decode(out[0][inputs['input_ids'].shape[1]:], skip_special_tokens=True).strip()

    def parse(answer):
        text = re.sub(r'^```(?:json)?\s*|\s*```$', '', answer.strip())
        try:
            value = json.loads(text)['association']
            return value if isinstance(value, str) else None
        except Exception:
            return None

    results = []
    for row in sample:
        card = json.loads(row['messages'][1]['content'].split('\n', 1)[1])
        raw = generate(row['messages'][:2])
        results.append({
            'target': row['meta']['target'],
            'method': row['meta']['method'],
            'text': card['text'],
            'meaning': card['meaning'],
            'raw': raw,
            'association': parse(raw),
            'reference': json.loads(row['messages'][2]['content'])['association'],
        })

    def rate(predicate):
        return f"{sum(1 for r in results if predicate(r)) / len(results):.0%}"

    first_word = lambda r: re.sub(r'[*.,!?¿¡]', '', r['text']).split()[0].lower()
    print('poprawny JSON:       ', rate(lambda r: r['association'] is not None))
    print('niepuste, ≤400 znaków:', rate(lambda r: bool(r['association']) and len(r['association']) <= 400))
    print('bez URL:             ', rate(lambda r: r['association'] is not None and 'http' not in r['association']))
    print('wymienia słowo:      ', rate(lambda r: bool(r['association']) and first_word(r) in r['association'].lower()))

    print("\n--- Przykładowe 5 skojarzeń obok wzorca ---")
    for r in results[:5]:
        print(f"[{r['target']}/{r['method']}] {r['text']} — {r['meaning']}")
        print('  model:  ', r['association'] if r['association'] is not None else '(niepoprawne) ' + r['raw'][:200])
        print('  wzorzec:', r['reference'])
        print()

    print("\n--> Scalanie wag LoRA z modelem bazowym...")
    merged = model.merge_and_unload()
    MERGED_DIR.mkdir(parents=True, exist_ok=True)
    merged.save_pretrained(str(MERGED_DIR), safe_serialization=True)
    tokenizer.save_pretrained(str(MERGED_DIR))
    del merged, model, trainer
    if torch.backends.mps.is_available():
        torch.mps.empty_cache()

    print("\n--> Konwersja do GGUF (f16)...")
    f16_gguf = TARGET_DIR / f"{RUN_NAME}.f16.gguf"
    quant_gguf = TARGET_DIR / f"{RUN_NAME}.{QUANT}.gguf"
    convert_script = LLAMA_CPP_DIR / 'convert_hf_to_gguf.py'

    subprocess.run([
        sys.executable,
        str(convert_script),
        str(MERGED_DIR),
        '--outtype', 'f16',
        '--outfile', str(f16_gguf),
    ], check=True)

    print(f"\n--> Kwantyzacja do {QUANT}...")
    quantize_bin = LLAMA_CPP_DIR / 'build' / 'bin' / 'llama-quantize'
    subprocess.run([
        str(quantize_bin),
        str(f16_gguf),
        str(quant_gguf),
        QUANT,
    ], check=True)

    print(f"Powstał plik skwantyzowany: {quant_gguf} ({quant_gguf.stat().st_size / (1024*1024):.1f} MB)")

    print("\n--> Szybki test generowania llama-cli...")
    cli_bin = LLAMA_CPP_DIR / 'build' / 'bin' / 'llama-cli'
    prompt = tokenizer.apply_chat_template(eval_rows[0]['messages'][:2], tokenize=False, add_generation_prompt=True)
    prompt_file = TARGET_DIR / 'test_prompt.txt'
    prompt_file.write_text(prompt, encoding='utf-8')

    test_res = subprocess.run([
        str(cli_bin),
        '-m', str(quant_gguf),
        '-f', str(prompt_file),
        '-n', '200',
        '--temp', '0.7',
        '-no-cnv',
        '-r', '<|im_end|>',
    ], capture_output=True, text=True)
    print("Wyjście testowe:")
    print("\n".join(test_res.stdout.splitlines()[-6:]))

    print("\n--> Generowanie model-manifest.json...")
    digest = hashlib.sha256()
    with open(quant_gguf, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            digest.update(chunk)

    manifest = {
        'id': RUN_NAME,
        'name': 'Bielik 1.5B (skojarzenia)',
        'file': quant_gguf.name,
        'url': f'https://github.com/{REPO}/releases/download/{RELEASE_TAG}/{quant_gguf.name}',
        'sha256': digest.hexdigest(),
        'bytes': quant_gguf.stat().st_size,
        'base': MODEL_ID,
        'license': 'Apache-2.0',
        'chatTemplate': 'chatml',
        'promptVersions': sorted({r['meta']['promptVersion'] for r in train_rows}),
        'contextTokens': 2048,
        'trainedAt': datetime.date.today().isoformat(),
        'trainExamples': len(train_rows),
    }

    manifest_file = TARGET_DIR / 'model-manifest.json'
    with open(manifest_file, 'w', encoding='utf-8') as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)

    print(f"Manifest zapisany w {manifest_file}:")
    print(json.dumps(manifest, indent=2, ensure_ascii=False))
    print(f"\n[SUKCES] Cały proces zakończony! Pliki gotowe w {TARGET_DIR}:")
    print(f"  - {quant_gguf}")
    print(f"  - {manifest_file}")


if __name__ == '__main__':
    main()
