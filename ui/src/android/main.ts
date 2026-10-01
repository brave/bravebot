// The Android entry: install `window.bravebot` before the renderer's first module reads it, then
// run the renderer unchanged. Import order is the guarantee, since ES modules evaluate in order.
import './bravebot'
import '../renderer/main'
