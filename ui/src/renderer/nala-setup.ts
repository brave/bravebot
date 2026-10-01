/**
 * Load Leo tokens and point icons at `./nala-icons` once per renderer entry.
 * Kept out of `nala.ts` so Node tests can import React wrappers without emitting CSS.
 */
import '@brave/leo/tokens/css/variables.css'
import { setIconBasePath } from './nala'

setIconBasePath('./nala-icons')
