# Brave Bot for Android

An Android shell around the same two pieces the desktop app is made of:

- **The agent**, from `crates/android`: `bravebot-ui-bridge` loaded in-process over JNI. It
  speaks the same newline-delimited JSON as `bravebot-rpc`, through `nativeSend` going in and
  `onLine` coming out instead of a pipe.
- **The renderer**, from `ui/src/renderer`, unchanged, in a WebView. `ui/src/android/bravebot.ts`
  provides `window.bravebot` over a WebView message channel. `Host.kt` does the main process's
  job on the other end: it checks every method against the desktop's allowlist and strips file
  lists from `turn.send`.

```
MainActivity ── WebView (https://appassets.androidplatform.net/assets/renderer/)
                  └─ window.bravebot ⇄ BravebotHost (Host.kt)
BravebotApp ─── Agent ⇄ NativeBridge ⇄ libbravebot_android.so (crates/android → ui-bridge)
```

## Build

Needs JDK 17, the Android SDK with an NDK, `cargo-ndk`, and the Rust targets
`aarch64-linux-android` and `x86_64-linux-android`.

```sh
./build-native.sh            # agent library into app/src/main/jniLibs, renderer into app/src/main/assets
./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

`build-native.sh` builds the Rust crate the way any other build does, so it needs the backend
configuration from `.envrc`, or `BRAVEBOT_ALLOW_UNCONFIGURED_BUILD=1` for a build that is
configured at run time.

## Configure a backend

The app sets `HOME` to its own files directory, so its state directory is
`/data/data/com.brave.bravebot/files/.bravebot`. To try a gateway on a debug build:

```sh
adb shell run-as com.brave.bravebot mkdir -p files/.bravebot
adb shell "run-as com.brave.bravebot sh -c 'cat > files/.bravebot/settings.json'" < settings.json
```

## What this build does not do yet

- **Layout:** the layout is still the desktop's three columns. There is no phone layout.
- **Workspace:** there is one project, `files/workspace`, and no folder picker.
- **Not wired up:** bots, forks, the file tree, attachments, export and native menus. The shim
  answers each with the desktop's empty value.
- **Commands:** nothing runs a command. The sandbox has no Android backend, so it reports
  itself unavailable and the agent refuses, failing closed.
- **Keys:** settings are a plain file. They are not in the Android Keystore yet.
- **Lifecycle:** a turn does not run in a foreground service, so it can be lost if the system
  kills the process in the background.
