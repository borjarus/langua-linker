import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// App variant from frontend/scripts/tauri-variant.mjs. The Kotlin package and Tauri identifier stay
// `com.mila.langualinker` (the CLI requires it); only the installed application id gets a suffix,
// so dev/beta/release install side by side with separate data. Unset (e.g. Android Studio) = dev.
// Debug builds: the Tauri CLI writes the suffix into the debug block from
// `bundle.android.debugApplicationIdSuffix` (tauri.<variant>.conf.json) and launches that package,
// so the suffix must not also come from defaultConfig. Release builds set it here.
val appVariant = System.getenv("LL_APP_VARIANT") ?: "dev"
require(appVariant in setOf("dev", "beta", "release")) { "Unknown LL_APP_VARIANT \"$appVariant\"" }

android {
    compileSdk = 34
    namespace = "com.mila.langualinker"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        manifestPlaceholders["appLabel"] = when (appVariant) {
            "dev" -> "LanguaLinker Dev"
            "beta" -> "LanguaLinker Beta"
            else -> "LanguaLinker"
        }
        applicationId = "com.mila.langualinker"
        minSdk = 26
        targetSdk = 34
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    // Release signing comes from the environment (CI secrets, see .github/workflows/release.yml);
    // the keystore never lives in the repository. Without it the release APK falls back to the
    // local debug key: installable for testing, but not upgradable across machines or for stores.
    val releaseKeystore = System.getenv("ANDROID_KEYSTORE_PATH")?.takeIf { it.isNotBlank() }
    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            applicationIdSuffix = ".beta"
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            applicationIdSuffix = if (appVariant == "release") null else ".$appVariant"
            signingConfig = signingConfigs.getByName(if (releaseKeystore != null) "release" else "debug")
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.6.1")
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("com.google.android.material:material:1.8.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")