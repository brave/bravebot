# Mobile app dependencies

The review [decision 9](decisions-and-tradeoffs.md#9-mobile-dependencies-and-builds) requires for the app in `mobile/`. It covers the direct dependencies, what an install and a build download, and what is and is not verified. It describes the tree at React Native 0.87.1 and is updated with each dependency change.

## Size

- 584 packages in `mobile/pnpm-lock.yaml`, of which 540 install on macOS arm64 (the rest are other platforms' optional packages).
- `node_modules` is 230 MB with the hoisted layout.
- The stock community template installs 864 packages and 308 MB with npm. The difference includes the template's lint, formatting and starter-screen packages, which the app does not use, and the two package managers resolve differently.
- The licences of the 540 installed packages are MIT (469), ISC (33), BSD-3-Clause (21), Apache-2.0 (9), BSD-2-Clause (3), and one each of Python-2.0 (`argparse`), CC-BY-4.0 (`caniuse-lite`, browser data), 0BSD (`jsc-safe-url`), MIT OR Apache-2.0 and MIT OR CC0-1.0 (`type-fest`). Nothing is copyleft. There is no licence policy check for npm packages in this tree yet.

## Direct dependencies

Licences and versions are from `pnpm licenses list` on the lockfile.

| Package | Version | Licence | Why | Native code |
|---|---|---|---|---|
| `react` | 19.2.3 | MIT | The renderer React Native 0.87.1 requires. | No |
| `react-native` | 0.87.1 | MIT | The framework. | Yes: C++, Objective-C and Kotlin. Delivered as prebuilt binaries (below), not built from source. |
| `react-native-safe-area-context` | 5.10.1 | MIT | Screen insets on both platforms. It is the only native module the app adds. | Yes: Kotlin and Objective-C++, with a `Package.swift` and a Gradle module. |
| `@brave/agent-client` | link | MPL-2.0 | The session client in `packages/agent-client`, linked rather than installed. It has no runtime dependencies. | No |

Development dependencies, all MIT except TypeScript (Apache-2.0):

| Package | Version | Why |
|---|---|---|
| `@babel/core` | 7.29.7 | Transforms the app and the linked client. |
| `@babel/plugin-transform-class-static-block` | 7.29.7 | `RpcAgentClient` uses a static class block that the React Native preset does not parse. |
| `@babel/runtime` | 7.29.10 | Helpers the transforms import. Resolved from the app's `node_modules` for the linked package. |
| `@react-native-community/cli`, `-platform-android`, `-platform-ios` | 20.2.0 | `react-native start`, `run-android`, autolinking configuration and `react-native spm`. |
| `@react-native/babel-preset`, `metro-config`, `jest-preset`, `typescript-config` | 0.87.1 | The React Native build, bundler and test configuration. |
| `jest`, `@types/jest` | 29.7.0, 29.5.14 | Tests. |
| `react-test-renderer`, `@types/react-test-renderer` | 19.2.3, 19.3.0 | Component tests. |
| `@types/react` | 19.3.0 | React types. |
| `typescript` | 6.0.3 | Type checking. |

The app adds no navigation library, gesture library, UI kit, state framework, analytics, remote JavaScript update service or Expo.

## Install and build scripts

No package in the lockfile has an install script (`requiresBuild` appears nowhere). `make check-mobile` installs with `--ignore-scripts` and passes.

## Downloads outside the lockfile

| Source | What | Verification |
|---|---|---|
| npm registry | Every package in the lockfile. | pnpm checks the sha512 integrity in the lockfile. `contrib/check-pnpm-lockfiles.py` accepts only registry sources with an integrity, and one link to `packages/agent-client`. |
| Maven Central (`repo1.maven.org`) | On iOS, the prebuilt React Native, React Native dependencies and Hermes XCFrameworks that `react-native spm` downloads into `~/Library/Caches/ReactNative` (about 1.9 GB for the debug and release flavors). | None found. The download script contains no checksum or signature check for the archives. |
| Maven Central and Google Maven | On Android, `com.facebook.react:react-android` and Hermes, the Android Gradle Plugin, the Kotlin plugin and AndroidX, resolved by Gradle. | None. The project has no Gradle dependency verification metadata. |
| `services.gradle.org` | The Gradle 9.4.1 distribution. | The wrapper validates the URL and the distribution's SHA-256 checksum. |
| Android SDK manager | NDK 27.1.12297006 and the platform tools, installed by the first build. | The SDK manager's own licence acceptance and checksums. |

The unverified rows are the gap Q5 keeps open. Closing it means pinning checksums for the iOS archives and enabling Gradle dependency verification.

## Advisories and licences

- `pnpm audit --audit-level=high` runs in CI for `mobile/` with the other lockfiles. It reports one high advisory, `GHSA-vfj7-8cjw-p6xm` in `braces` (no patched release), reached only through Metro and the React Native CLI, which are build-time tooling that is not in the app bundle. It is recorded in `pnpm-workspace.yaml` as `ui/` records the same advisory. A moderate advisory in `sprintf-js` (`GHSA-hp3w-g68c-fv3c`, no patched release) does not fail the audit.
- No advisory or licence check covers the Gradle, Maven or SwiftPM dependencies.

## Checks

`make check-npm` lints `mobile/pnpm-lock.yaml` and checks it against `package.json`. `make check-mobile` type-checks and tests the app. `contrib/affected-checks.py` gives a change under `mobile/` the `mobile` area, which runs `check-mobile` in the `Mobile app` job, and also the `npm` area when the change is to `package.json` or a lockfile, which runs `check-npm`. `.github/CODEOWNERS` covers the manifest, lockfile, Metro and Babel configuration, and the Gradle and Xcode project entry points. Native builds are not in CI.
