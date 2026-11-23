// Keep batch entry points on the same .env/environment/discovery path as builds.
import { VCVARS, require } from './paths.mjs';
console.log(require('VCVARS', VCVARS, 'Visual Studio C++ build tools'));
