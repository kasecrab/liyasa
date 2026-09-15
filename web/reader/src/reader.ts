// The bundle the theme loads with the page. One job so far; the rest of the
// runtime is still vendored in `crates/liyasa-theme/assets/js/`
// (`plan/rfcs/1100-reader-toolchain.md`).

import { install } from "./navigate.ts";

install(window as never);
