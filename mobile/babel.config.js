module.exports = {
  presets: ['module:@react-native/babel-preset'],
  // RpcAgentClient uses a static class block, which the preset does not parse.
  plugins: ['@babel/plugin-transform-class-static-block'],
};
