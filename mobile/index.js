/**
 * @format
 */

import { AppRegistry } from 'react-native';
import App from './src/App';
import { name as appName } from './app.json';

// Only a development build can reach the development relay.
const connect = __DEV__ ? require('./src/devRelay').connectDevRelayTarget : undefined;

AppRegistry.registerComponent(appName, () => () => <App connect={connect} />);
