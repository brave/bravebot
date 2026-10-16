module.exports = {
  preset: '@react-native/jest-preset',
  // The linked client package is ESM and sits outside node_modules, so it is transformed like app code.
  // Its injected @babel/runtime imports resolve from the package's own directory, so the app's modules are added.
  modulePaths: ['<rootDir>/node_modules'],
  testMatch: ['**/*.test.[jt]s?(x)'],
  testPathIgnorePatterns: ['/node_modules/', '/android/', '/ios/'],
};
