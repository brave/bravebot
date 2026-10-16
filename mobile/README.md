# Bravebot mobile

The React Native app described in [docs/design/mobile/app-shell.md](../docs/design/mobile/app-shell.md). It is a shell that builds and runs on both platforms and shows a blank slate, because no target is connected yet. It is a bare React Native 0.87.1 app (React 19.2.3, Hermes) that consumes [`packages/agent-client`](../packages/agent-client/README.md) by link. Its one native dependency beyond React Native is `react-native-safe-area-context`. The dependency review is [docs/design/mobile/app-dependencies.md](../docs/design/mobile/app-dependencies.md).

## Requirements

- Node `^22.13.0 || ^24.3.0 || >= 26.0.0` and pnpm (`corepack enable` provides the version `package.json` pins).
- Android: JDK 17 and the Android SDK (`ANDROID_HOME` set). The first Gradle build installs NDK 27.1.12297006 into the SDK.
- iOS: Xcode and a simulator. No CocoaPods, Bundler or Ruby is needed to build or run.

## Install and check

```sh
pnpm --dir ../packages/agent-client install --frozen-lockfile --ignore-scripts
pnpm --dir ../packages/agent-client run build   # the app resolves the client through its dist
pnpm install --frozen-lockfile
pnpm run check                                  # tsc --noEmit, then jest
```

`make check-mobile` from the repository root does the same. It builds no native code.

`pnpm-workspace.yaml` sets `nodeLinker: hoisted`. Gradle includes `@react-native/gradle-plugin` from `../node_modules`, a package react-native depends on rather than the app, and pnpm's default layout does not put it there. Metro and jest work with either layout.

## Release builds

A debug build asks for the network because it fetches its JavaScript from Metro, and a release build asks for none, because the app has no network target. Add `INTERNET` to the main Android manifest with the first target that needs it. The release variant is signed only with a key supplied in the Gradle properties `BRAVEBOT_RELEASE_STORE_FILE`, `BRAVEBOT_RELEASE_STORE_PASSWORD`, `BRAVEBOT_RELEASE_KEY_ALIAS` and `BRAVEBOT_RELEASE_KEY_PASSWORD`, for example in `~/.gradle/gradle.properties`. Without them it is left unsigned, so the template's public debug key never signs a release. Neither platform has a release or distribution build tested yet.

## App icon

Both apps use the desktop app's icon, the robot drawn in `ui/build/icon.svg`. `scripts/make-icons.sh` cuts it for each platform and writes the files under `android/` and `ios/`, so after the drawing changes run it again and commit the result. It needs macOS, because it renders with AppKit (`swiftc`) and flattens with `sips`. The desktop's own recipe in `ui/docs/development.md` uses Inkscape for the same drawing.

- iOS: one 1024 point square with no alpha channel, in `ios/Bravebot/Images.xcassets/AppIcon.appiconset`. The system rounds it.
- Android 8 and later: an adaptive icon. The drawing sits at two thirds of the foreground layer, over a background of the tile's own gradient, so any mask the launcher applies shows the whole robot.
- Android before 8: the rounded tile and a round variant, in `mipmap-*dpi`.

## Consuming the client package

`@brave/agent-client` is a `link:` dependency. Three settings make it work and the package itself is unchanged:

- `metro.config.js` watches `../packages/agent-client`, because Metro does not follow a link whose target is outside the app root.
- `metro.config.js` and `jest.config.js` add the app's `node_modules` to module lookup, because Babel's injected `@babel/runtime` imports resolve from the package's own directory.
- `babel.config.js` enables `@babel/plugin-transform-class-static-block`, because `RpcAgentClient` uses a static class block that the React Native preset does not parse.

## Run on Android

```sh
pnpm start                                        # Metro, in one terminal
adb reverse tcp:8081 tcp:8081
pnpm android                                      # builds, installs and launches (add --deviceId <id> for a specific device)
```

A debug build on Android 17 and later declares `ACCESS_LOCAL_NETWORK`, and the app cannot reach Metro until that permission is granted. Allow the prompt, or run `adb shell pm grant com.brave.bravebot android.permission.ACCESS_LOCAL_NETWORK`. Earlier Android versions have no such permission and the grant command fails harmlessly. The app is portrait only.

## Run on the iOS simulator

```sh
pnpm start                                        # Metro, in one terminal
xcrun simctl boot <simulator id>
xcodebuild -project ios/Bravebot.xcodeproj -scheme Bravebot -configuration Debug \
  -destination 'id=<simulator id>' -derivedDataPath /tmp/bravebot-dd CODE_SIGNING_ALLOWED=NO build
xcrun simctl install <simulator id> /tmp/bravebot-dd/Build/Products/Debug-iphonesimulator/Bravebot.app
xcrun simctl launch <simulator id> com.brave.bravebot
```

Building runs the project's "Sync SPM Autolinking" phase, which regenerates `ios/build/` (ignored by git) and downloads the prebuilt React Native and Hermes XCFrameworks into `~/Library/Caches/ReactNative` on first use. A device build needs a development team set in Xcode. The app is portrait only on iPhone and iPad (`UIRequiresFullScreen` is set so iPad can be portrait only).

Do not run `pod install`. `ios/Podfile` is a placeholder with no dependencies, kept because the React Native CLI finds the iOS project through a file of that name. After `npx react-native spm update` check that `HERMES_CLI_PATH` in `ios/Bravebot.xcodeproj/project.pbxproj` is still `$(SRCROOT)/../node_modules/hermes-compiler/hermesc/osx-bin/hermesc`, because the tool writes an absolute path.

## How the iOS project was made

The community template produces a CocoaPods project, and `npx react-native spm add` refuses one until it is deintegrated. The conversion was done once and the result is committed, so nobody else repeats it. The steps, for recreating the project:

1. Generate the template into a scratch directory and copy it over `mobile/`:

   ```sh
   npx @react-native-community/cli@latest init Bravebot --version 0.87.1 --skip-install --skip-git-init \
     --install-pods false --package-name com.brave.bravebot --title Bravebot
   ```

2. Install dependencies with `pnpm install` (hoisted layout).
3. Remove the CocoaPods wiring from the Xcode project with the `xcodeproj` gem, which installs on the system Ruby without CocoaPods (`GEM_HOME=<dir> gem install xcodeproj --no-document`):

   ```ruby
   require 'xcodeproj'
   proj = Xcodeproj::Project.open('ios/Bravebot.xcodeproj')
   proj.targets.each do |t|
     t.build_configurations.each do |c|
       ref = c.base_configuration_reference
       c.base_configuration_reference = nil if ref && ref.path.to_s =~ /Pods-.*\.xcconfig/
     end
     t.build_phases.dup.each { |p| p.remove_from_project if p.respond_to?(:name) && p.name.to_s.start_with?('[CP]') }
   end
   proj.files.dup.each { |f| f.remove_from_project if f.path.to_s =~ /Pods-.*\.(xcconfig|debug|release)|libPods|Pods_.*\.framework/ }
   proj.main_group.children.dup.each { |g| g.remove_from_project if g.display_name == 'Pods' && g.children.empty? }
   proj.save
   ```

4. Run `npx react-native spm add --yes` from `mobile/`. It injects the Swift package references, the "Sync SPM Autolinking" build phase and `ios/Bravebot.xcodeproj/.spm-injected.json` (committed, it records what `spm deinit` removes).
5. Replace the contents of `ios/Podfile` with the placeholder comment, delete `Gemfile`, and set the iPad orientations to portrait with `UIRequiresFullScreen` in `ios/Bravebot/Info.plist`.
6. Make `HERMES_CLI_PATH` relative as described above.

Generated and cached output stays out of git: `ios/build/`, `android/build/`, `android/app/build/`, `android/.gradle/`, `node_modules/`.
