import { registerRootComponent } from 'expo';

import App from './App';
// Tache de position en arriere-plan : a definir dans la portee globale, avant tout rendu.
import './src/location/sharing';

// registerRootComponent calls AppRegistry.registerComponent('main', () => App);
// It also ensures that whether you load the app in Expo Go or in a native build,
// the environment is set up appropriately
registerRootComponent(App);
