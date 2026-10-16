import { SafeAreaProvider } from 'react-native-safe-area-context'
import { NoTargetScreen } from './screens/NoTargetScreen'

/** The app shell. It offers no target yet, so it shows the blank slate. */
export default function App() {
  return (
    <SafeAreaProvider>
      <NoTargetScreen />
    </SafeAreaProvider>
  )
}
