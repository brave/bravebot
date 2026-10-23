const path = require('path');
const { getDefaultConfig, mergeConfig } = require('@react-native/metro-config');

// Metro does not follow the linked package outside the app root unless it is watched, and
// Babel's injected @babel/runtime imports resolve from the package's own directory.
module.exports = mergeConfig(getDefaultConfig(__dirname), {
  watchFolders: [path.resolve(__dirname, '../packages/agent-client')],
  resolver: { nodeModulesPaths: [path.resolve(__dirname, 'node_modules')] },
});
