/**
 * Where the two helper binaries are.
 *
 * `bravebot-rpc` and `bravebot-ui-files` are built by one cargo workspace and shipped the same
 * way, so the composition is here rather than spelled out beside each of them. The reason it is
 * worth a module of its own is the extension: Windows names an executable with `.exe`, and a
 * suffix added at one of the two call sites is a helper that is found in a checkout and not in a
 * packaged app, or the other way round, which nothing short of running the app on Windows would
 * show.
 */

import { join } from 'node:path'

/** The two places one helper can be, for a caller that decides between them. */
export interface HelperPaths {
  /** Beside a packaged app, as a resource. */
  packaged: string
  /** Wherever `cargo build` last left it. The app path is `ui/`; the workspace is above it. */
  development: string
}

export function helperPaths(
  name: string,
  appPath: string,
  resourcesPath: string = process.resourcesPath ?? '',
  platform: NodeJS.Platform = process.platform,
): HelperPaths {
  const file = platform === 'win32' ? `${name}.exe` : name
  return {
    packaged: join(resourcesPath, file),
    development: join(appPath, '..', 'target', 'debug', file),
  }
}
