// The collector as a loadable script: `e2e/vitals.spec.ts` adds it before the
// page runs and reads `window.__liyasaVitals` after. It is not part of the
// bundle a reader loads.

import { collect } from "./vitals.ts";

(window as never as Record<string, unknown>)["__liyasaVitals"] = collect(window as never);
