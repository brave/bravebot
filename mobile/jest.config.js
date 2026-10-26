module.exports = {
  preset: '@react-native/jest-preset',
  // The linked client package is ESM and sits outside node_modules, so it is transformed like app code.
  // Its injected @babel/runtime imports resolve from the package's own directory, so the app's modules are added.
  modulePaths: ['<rootDir>/node_modules'],
  // CI starts with an empty transform cache, so the first test of a file that renders the app compiles
  // react-native's modules while it runs. That takes about 2.5 s on an idle machine and more than the
  // default 5 s when the runner is busy. A test that hangs still fails, after this limit.
  testTimeout: 30000,
  testMatch: ['**/*.test.[jt]s?(x)'],
  testPathIgnorePatterns: ['/node_modules/', '/android/', '/ios/'],
};
