# Do zrobienia na drugim laptopie

Stan na 2026-11-20: wydane **v0.5.2-beta.1** (`main` = `d87781e`). Ten plik nie jest w gicie. Skasuj go, gdy wszystko będzie zrobione.

## 1. Przygotowanie laptopa

1. `git clone https://github.com/borjarus/langua-linker.git` (albo `git pull` na `main`) i sprawdź, czy tag `v0.5.2-beta.1` jest na miejscu.
2. Plik `.env` w katalogu głównym repo z `LANGUA_DECKS_PASSWORD=…`. Przenieś go bezpiecznie, np. z menedżera haseł, nie mailem. Bez niego talie się nie rozpakują.
3. Narzędzia:
   - Rust z rustup; wersję (1.90.0) wybierze `rust-toolchain.toml`. Do tego targety Androida: `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`.
   - Node 24, potem `npm --prefix frontend ci`.
   - JDK 17, Android SDK, **NDK 27.3.13750724** (ten sam co w CI).
   - **CMake i LLVM (libclang)**. Bez nich build na Androida się nie uda, bo kompiluje llama.cpp (ADR-23).
   - Na Windows: Visual Studio z kompletnym Windows SDK. Na pierwszym PC brakowało `dbghelp.lib`, przez co nie linkowało się nic w Rust.
4. Zmienne środowiskowe dla buildu na Androida (tak jak w CI):
   - `ANDROID_NDK` = `<Android SDK>/ndk/27.3.13750724`
   - `ANDROID_API_LEVEL` = `26`
5. Sprawdzenie: `cargo test --workspace` oraz `npm --prefix frontend run check`, `lint` i `test`. Potem `npm --prefix frontend run android:apk`, który powinien zbudować się razem z llama.cpp.

## 2. Model skojarzeń na telefonie (ADR-23, kroki 4–5)

1. **Dane treningowe:** `cargo run -p langualinker-application --bin llm-dataset -- content/decks target/llm-dataset`. Powstaną `train.jsonl` (4 753 przykłady) i `eval.jsonl` (271). Zawierają treść talii, więc nie publikuj ich.
2. **Hugging Face:** zaakceptuj warunki modelu `speakleash/Bielik-1.5B-v3.0-Instruct` i utwórz token *read*.
3. **Colab:** otwórz `content/tools/finetune/bielik_associations.ipynb`, ustaw GPU T4, dodaj sekret `HF_TOKEN`, uruchom kolejne komórki i wgraj oba pliki `.jsonl`. Trening trwa ok. 30–60 min.
4. **Ocena:** przejrzyj wyniki z sekcji 6 notatnika (poprawny JSON, długość, czy skojarzenie wymienia słowo) i 25 przykładów obok wzorca. Jeśli jakość jest za słaba, nie wydawaj modelu: zmień liczbę epok lub rank LoRA, albo dołóż dane.
5. **Publikacja** (decyzja: GitHub Releases, publicznie): pobierz `bielik-1.5b-associations-v1.Q4_K_M.gguf` i `model-manifest.json`, potem
   `gh release create model-bielik-associations-v1 <plik .gguf> model-manifest.json --repo borjarus/langua-linker --prerelease --title "On-device association model v1"`
6. **Katalog w aplikacji:** dopisz manifest do tablicy `models` w `content/models/catalog.json`. Test `the_compiled_catalog_parses` sprawdza, czy wpis jest poprawny. Dopiero wtedy dostawca „Na urządzeniu” pojawi się na Androidzie.
7. **Pomiary na Galaxy A56** z prawdziwym modelem: czas ładowania, czas odpowiedzi (cel poniżej 4 s dla słowa), pamięć (budżet ok. 1,5 GB), bateria na 20 generacji. Jeśli będzie za wolno, spróbuj kwantyzacji `Q4_0` lub 3-bitowej. Wyniki wpisz do ADR-23, a jego status zmień na *Accepted*.
8. Nowa wersja aplikacji z modelem w katalogu: podbij wersję (`node scripts/release-check.mjs --tag v…` sprawdzi spójność), dopisz wpis w CHANGELOG, potem tag.

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
