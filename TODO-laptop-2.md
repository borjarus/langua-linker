# Do zrobienia na drugim laptopie

Stan na 2026-11-20: wydane **v0.5.2-beta.1** (`main` = `d87781e`). Ten plik nie jest w gicie. Skasuj go, gdy wszystko będzie zrobione.

## 1. Przygotowanie laptopa (ZROBIONE)

1. [x] `git clone` / `git pull` na `main` i sprawdzenie tagu `v0.5.2-beta.1`.
2. [x] Plik `.env` w katalogu głównym repo ze sprostowanym `LANGUA_DECKS_PASSWORD` (usunięto przypadkowe `ś` na końcu).
3. [x] Narzędzia:
   - Rust 1.90.0 z targetami: `aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.
   - Node 26 + `npm --prefix frontend ci`.
   - JDK 17, Android SDK, NDK 27.3.13750724.
   - CMake 4.5.1 (Homebrew) + Clang / LLVM.
4. [x] Zmienne środowiskowe `ANDROID_NDK` i `ANDROID_API_LEVEL=26`.
5. [x] Sprawdzenie: `cargo test --workspace`, `npm run check`, `lint`, `test` oraz `npm --prefix frontend run android:apk` (zbudowano APK debug z llama.cpp).

## 2. Model skojarzeń na telefonie (ADR-23, kroki 4–5)

1. [x] **Dane treningowe:** wygenerowano `train.jsonl` (4 753) i `eval.jsonl` (271) w `target/llm-dataset`.
2. [x] **Hugging Face:** zaakceptowano warunki modelu `speakleash/Bielik-1.5B-v3.0-Instruct`, dodano `HF_TOKEN` do `.env`.
3. [x] **Trening lokalny na GPU (Apple M3 Max / MPS):** wykonano skryptem `content/tools/finetune/train_local.py`:
   - trening LoRA na GPU (Metal/MPS bfloat16);
   - ewaluacja jakościowa na 120 przykładach (JSON: 100%, długość: 100%, bez URL: 100%, wymienia słowo: 72%);
   - scalenie wag LoRA z modelem bazowym;
   - konwersja do GGUF i kwantyzacja do `bielik-1.5b-associations-v1.Q4_K_M.gguf` (927,7 MB);
   - wygenerowanie `target/model-manifest.json` (SHA-256: `a74abb91fbf7ba16aa316a1daefa3236ae2c51086ebb524a5bbf6cddf65aa0e9`).
4. [x] **Ocena:** jakość skojarzeń zweryfikowana (odpowiedzi zwięzłe, merytoryczne, bez halucynacji URL).
5. [x] **Publikacja:** opublikowano release na GitHubie:
   `https://github.com/borjarus/langua-linker/releases/tag/model-bielik-associations-v1`
   zawierający plik GGUF (927,7 MB) oraz `model-manifest.json`.
6. [x] **Katalog w aplikacji:** manifest dopisany do `content/models/catalog.json`. Test `the_compiled_catalog_parses` oraz cały pakiet testów przechodzi pomyślnie.
7. [x] **Pomiary na Galaxy A56** z prawdziwym modelem:
   - Czas ładowania: ~1,75 s (mmap do RAM).
   - Czas odpowiedzi: średnio ~21–23 s dla skojarzenia słowa (~1,8–2,0 tokenów/s na 4 wątkach CPU).
   - Pamięć: szczytowe PSS ~1 246 MB / RSS ~1 322 MB, spoczynkowe PSS ~1 180 MB (w budżecie 1,5 GB z 250 MB zapasu).
   - Bateria na 20 generacji: spadek 3% (155 mAh, ~7,75 mAh na generację), temp. wzrosła z 30,7°C do 34,1°C (+3,4°C).
   - Jakość: 20/20 poprawny JSON, zwięzłe polskie mnemotechniki, brak halucynacji URL, pozytywna moderacja.
   - Wyniki wpisane do ADR-23, status zmieniony na **Accepted**.
8. [x] **Nowa wersja aplikacji z modelem w katalogu:**
   - Podbito wersję do `0.5.2-beta.2` (`versionCode = 50232`, `wix = 0.5.2.2`).
   - Wpis dodany w `CHANGELOG.md`.
   - Zweryfikowano: `node scripts/release-check.mjs --tag v0.5.2-beta.2` przeszło pomyślnie.
   - Wszystkie testy (`cargo test --workspace`, `svelte-check`, `npm test`) przeszły pomyślnie.

## 3. Znane drobne problemy do rozważenia

- Przy pierwszym starcie po migracji bazy WebView loguje `unhandledRejection` (było już w 0.5.1). Warto znaleźć źródło.
- Dodawanie słowa z niemieckiego zdania zostawia wielką literę wyrazu z początku zdania („Wir” zamiast „wir”). Pole można poprawić ręcznie.
- Nazwy talii („Angielski A1 — zdania”) nie są tłumaczone na angielski. Tłumaczone są tylko wskazówki i rozdziały gramatyki.
- iOS i desktop nie mają silnika modelu. Włączyć dopiero po pomiarach (`src-tauri/Cargo.toml`, blok dla `target_os = "android"`).
- APK z wydania na GitHubie ma inny podpis niż lokalne buildy debug. Beta zainstalowana na telefonie z lokalnego buildu nie zaktualizuje się nim bez odinstalowania. Zrób najpierw kopię kolekcji (Ustawienia → Kopia zapasowa).

## 4. Sprzątanie po pierwszym PC

Zrobione 2026-11-20: usunięta gałąź `release/0.5.2-beta.1` (lokalnie i na GitHubie), kopia kolekcji z telefonu i zrzuty ekranu. Git nie ma już zarejestrowanej kopii roboczej `C:\ll-build`.

Do zrobienia ręcznie (Claude nie może kasować katalogów w korzeniu `C:\`):
- usuń resztki katalogu `C:\ll-build` w Eksploratorze;
- opcjonalnie usuń cache buildów `C:\ll-target` i `C:\ll-target-test`, żeby zwolnić miejsce; odbudują się same.
