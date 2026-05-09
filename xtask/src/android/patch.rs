use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;

use crate::util::{exe, root, sh};

use super::paths::{abi_for, android_home, gen_android};

pub(super) fn patch_gradle_properties() -> Result<()> {
    let path = gen_android().join("gradle.properties");
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&path)?;
    let mut next = raw
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.starts_with("org.gradle.configureondemand=")
                && !line.starts_with("org.gradle.warning.mode=")
                && !line.starts_with("org.gradle.problems.report=")
        })
        .collect::<Vec<_>>()
        .join("\n");
    next.push_str("\norg.gradle.warning.mode=none\norg.gradle.problems.report=false\n");
    if next != raw {
        fs::write(path, next)?;
    }
    Ok(())
}

pub(super) fn patch_gradle_build_config() -> Result<()> {
    let gradle = gen_android().join("app").join("build.gradle.kts");
    if !gradle.exists() {
        return Ok(());
    }
    let mut raw = fs::read_to_string(&gradle)?;
    if !raw.contains("val wtDevApk =") {
        raw = raw.replace(
            "val tauriProperties = Properties().apply {\n    val propFile = file(\"tauri.properties\")\n    if (propFile.exists()) {\n        propFile.inputStream().use { load(it) }\n    }\n}",
            "val tauriProperties = Properties().apply {\n    val propFile = file(\"tauri.properties\")\n    if (propFile.exists()) {\n        propFile.inputStream().use { load(it) }\n    }\n}\n\nval wtDevApk = (project.findProperty(\"wtDevApk\") as? String == \"true\") || (System.getenv(\"WT_DEV_APK\") == \"1\")",
        );
    }
    raw = raw.replace(
        "isDebuggable = (project.findProperty(\"wtDevApk\") as? String == \"true\") || (System.getenv(\"WT_DEV_APK\") == \"1\")",
        "isDebuggable = wtDevApk",
    );
    raw = raw.replace("isMinifyEnabled = true", "isMinifyEnabled = !wtDevApk");
    if !raw.contains("sourceCompatibility = JavaVersion.VERSION_17") {
        raw = raw.replace(
            "    kotlinOptions {\n        jvmTarget = \"1.8\"\n    }",
            "    compileOptions {\n        sourceCompatibility = JavaVersion.VERSION_17\n        targetCompatibility = JavaVersion.VERSION_17\n    }\n    kotlinOptions {\n        jvmTarget = \"17\"\n        suppressWarnings = true\n    }",
        );
    }
    if raw.contains("jvmTarget = \"17\"") && !raw.contains("suppressWarnings = true") {
        raw = raw.replace(
            "        jvmTarget = \"17\"",
            "        jvmTarget = \"17\"\n        suppressWarnings = true",
        );
    }
    if !raw.contains("jniLibs.useLegacyPackaging = true") {
        raw = raw.replace(
            "    buildFeatures {\n        buildConfig = true\n    }",
            "    buildFeatures {\n        buildConfig = true\n    }\n    packaging {\n        jniLibs.useLegacyPackaging = true\n    }",
        );
    }
    fs::write(&gradle, raw)?;
    Ok(())
}

pub(super) fn sign_with_debug_keystore(unsigned: &Path, signed: &Path) -> Result<()> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let ks = Path::new(&home).join(".android").join("debug.keystore");
    if !ks.exists() {
        bail!("debug.keystore not found at {}", ks.display());
    }
    let bt_dir = android_home().join("build-tools");
    let bt_ver = fs::read_dir(&bt_dir)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .max()
        .context("no Android build-tools installed")?;
    let bt = bt_dir.join(bt_ver);
    let zipalign = bt.join(exe("zipalign"));
    let apksigner = bt.join(if cfg!(windows) {
        "apksigner.bat"
    } else {
        "apksigner"
    });
    let aligned = unsigned.with_file_name("app-universal-release-aligned.apk");
    sh(
        &zipalign.to_string_lossy(),
        &[
            "-f",
            "-p",
            "4",
            &unsigned.to_string_lossy(),
            &aligned.to_string_lossy(),
        ],
    )?;
    sh(
        &apksigner.to_string_lossy(),
        &[
            "sign",
            "--ks",
            &ks.to_string_lossy(),
            "--ks-pass",
            "pass:android",
            "--ks-key-alias",
            "androiddebugkey",
            "--key-pass",
            "pass:android",
            "--out",
            &signed.to_string_lossy(),
            &aligned.to_string_lossy(),
        ],
    )?;
    let _ = fs::remove_file(&aligned);
    Ok(())
}

pub(super) fn copy_llama_jni(target: &str) -> Result<()> {
    let abi = abi_for(target)?.abi;
    let llama_src = root()
        .join("src-tauri")
        .join("jniLibs")
        .join(abi)
        .join("libllama-cli.so");
    let gen_dir = gen_android()
        .join("app")
        .join("src")
        .join("main")
        .join("jniLibs")
        .join(abi);
    if llama_src.exists() && gen_dir.exists() {
        fs::create_dir_all(&gen_dir)?;
        fs::copy(&llama_src, gen_dir.join("libllama-cli.so"))?;
    }
    Ok(())
}

pub(super) fn patch_manifest() -> Result<()> {
    apply_android_overlay()?;
    let main = gen_android().join("app").join("src").join("main");
    let p = main.join("AndroidManifest.xml");
    if !p.exists() {
        return Ok(());
    }
    let mut raw = fs::read_to_string(&p)?;
    raw = raw.replace("\n        android:extractNativeLibs=\"true\"", "");
    if !raw.contains("android.permission.WAKE_LOCK") {
        let perms = concat!(
            "    <uses-permission android:name=\"android.permission.WAKE_LOCK\" />\n",
            "    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE\" />\n",
            "    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE_DATA_SYNC\" />\n",
            "    <uses-permission android:name=\"android.permission.POST_NOTIFICATIONS\" />\n",
            "    <uses-permission android:name=\"android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS\" />\n",
            "    <uses-feature",
        );
        raw = raw.replacen("    <uses-feature", perms, 1);
    }
    if !raw.contains(".TranscriptionService") {
        let service = concat!(
            "        <service\n",
            "            android:name=\".TranscriptionService\"\n",
            "            android:exported=\"false\"\n",
            "            android:foregroundServiceType=\"dataSync\" />\n\n",
            "        <provider",
        );
        raw = raw.replacen("        <provider", service, 1);
    }
    fs::write(&p, raw)?;
    Ok(())
}

const MAIN_ACTIVITY_KT: &str = include_str!(
    "../../../src-tauri/android-overlay/java/com/asolopovas/wtranscriber/MainActivity.kt"
);
const TRANSCRIPTION_SERVICE_KT: &str = include_str!(
    "../../../src-tauri/android-overlay/java/com/asolopovas/wtranscriber/TranscriptionService.kt"
);
const STRINGS_XML: &str =
    include_str!("../../../src-tauri/android-overlay/res/values/strings.xml");

fn apply_android_overlay() -> Result<()> {
    let main = gen_android().join("app").join("src").join("main");
    if !main.exists() {
        return Ok(());
    }
    let java_dir = main
        .join("java")
        .join("com")
        .join("asolopovas")
        .join("wtranscriber");
    let res_dir = main.join("res").join("values");
    write_if_changed(&java_dir.join("MainActivity.kt"), MAIN_ACTIVITY_KT)?;
    write_if_changed(
        &java_dir.join("TranscriptionService.kt"),
        TRANSCRIPTION_SERVICE_KT,
    )?;
    write_if_changed(&res_dir.join("strings.xml"), STRINGS_XML)?;
    apply_android_icons(&main.join("res"))?;
    Ok(())
}

fn apply_android_icons(res: &Path) -> Result<()> {
    let icons = root().join("src-tauri").join("icons").join("android");
    if !icons.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&icons)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dst_dir = res.join(entry.file_name());
        fs::create_dir_all(&dst_dir)?;
        for file in fs::read_dir(entry.path())? {
            let file = file?;
            if file.file_type()?.is_file() {
                copy_if_changed(&file.path(), &dst_dir.join(file.file_name()))?;
            }
        }
    }
    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::read_to_string(path).is_ok_and(|existing| existing == content) {
        return Ok(());
    }
    fs::write(path, content)?;
    Ok(())
}

fn copy_if_changed(src: &Path, dst: &Path) -> Result<()> {
    if let (Ok(a), Ok(b)) = (fs::read(src), fs::read(dst))
        && a == b
    {
        return Ok(());
    }
    fs::copy(src, dst)?;
    Ok(())
}

pub fn sign_patch_inline() -> Result<i32> {
    let gradle = gen_android().join("app").join("build.gradle.kts");
    if !gradle.exists() {
        println!(
            "sign-patch: gen/android not found — run `xtask android prebuilts` + tauri android init first"
        );
        return Ok(0);
    }
    let marker = "// wtranscriber: keystore-signing-patch v2";
    let raw = fs::read_to_string(&gradle)?;
    if raw.contains(marker) {
        println!("sign-patch: already patched");
        return Ok(0);
    }
    let raw = if raw.contains("// wtranscriber: keystore-signing-patch v1") {
        println!("sign-patch: superseding v1 patch");
        raw.replace(
            "val keystoreProperties = java.util.Properties()",
            "val keystoreProperties = Properties()",
        )
        .replace(
            "// wtranscriber: keystore-signing-patch v1",
            "// wtranscriber: keystore-signing-patch v2",
        )
    } else {
        raw
    };
    if raw.contains("// wtranscriber: keystore-signing-patch v2") {
        fs::write(&gradle, &raw)?;
        println!("sign-patch: refreshed to v2");
        return Ok(0);
    }
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = raw.split('\n').collect();
    let find_line = |start: usize, pred: &dyn Fn(&str) -> bool| -> Option<usize> {
        (start..lines.len()).find(|&i| pred(lines[i].trim_end_matches('\r')))
    };
    let Some(android_idx) = find_line(0, &|l| l.starts_with("android {")) else {
        println!("sign-patch: `android {{` block not found — skipping");
        return Ok(0);
    };
    let load_props: Vec<String> = [
        format!("    {marker}"),
        "    val keystorePropertiesFile = rootProject.file(\"keystore.properties\")".into(),
        "    val keystoreProperties = Properties()".into(),
        "    if (keystorePropertiesFile.exists()) {".into(),
        "        keystorePropertiesFile.inputStream().use { keystoreProperties.load(it) }".into(),
        "    }".into(),
        "    signingConfigs {".into(),
        "        register(\"release\") {".into(),
        "            if (keystorePropertiesFile.exists()) {".into(),
        "                storeFile = file(keystoreProperties[\"storeFile\"] as String)".into(),
        "                storePassword = keystoreProperties[\"storePassword\"] as String".into(),
        "                keyAlias = keystoreProperties[\"keyAlias\"] as String".into(),
        "                keyPassword = keystoreProperties[\"keyPassword\"] as String".into(),
        "            }".into(),
        "        }".into(),
        "    }".into(),
    ]
    .into();
    let release_idx = find_line(android_idx, &|l| {
        let t = l.trim();
        t.starts_with("getByName(\"release\")") || t.starts_with("release {")
    });
    let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    new_lines.splice((android_idx + 1)..(android_idx + 1), load_props.clone());
    if let Some(rel_idx) = release_idx.map(|i| i + load_props.len())
        && !new_lines[rel_idx].contains("signingConfig")
    {
        new_lines.insert(
            rel_idx + 1,
            "            signingConfig = signingConfigs.getByName(\"release\")".into(),
        );
    }
    let mut joined = new_lines.join("\n");
    if eol == "\r\n" {
        joined = joined.replace('\n', "\r\n");
    }
    fs::write(&gradle, joined)?;
    println!("sign-patch: applied");
    Ok(0)
}
