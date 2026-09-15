(function () {
"use strict";

// RX-04: page changes animate with the View Transitions API where the browser
// has one.
//
// Liyasa ships no client router, so nothing here intercepts a click or fetches
// a page: the transition is *declared*, and the browser runs it across an
// ordinary navigation. That keeps the back button, the network cache, and a
// JavaScript-free reader on exactly the same code path.

const RULE = "@view-transition { navigation: auto; }";

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

                     
                                  
 

                     
                   
                                                              
 

                        
                                  
                                
 

                                 
                         
                                      
                                  
                                        
 

function supported(win                )          {
  if (typeof win.CSSStyleSheet !== "function") return false;
  if (!Array.isArray(win.document.adoptedStyleSheets)) return false;
  // `CSSViewTransitionRule` is the cross-document half; `startViewTransition`
  // alone means a browser that would honour the rule once it ships it.
  return (
    typeof win.CSSViewTransitionRule !== "undefined" ||
    typeof win.document.startViewTransition === "function"
  );
}

function install(win                )          {
  const Sheet = win.CSSStyleSheet;
  if (!supported(win) || typeof Sheet !== "function") return false;

  const doc = win.document;
  const sheet = new Sheet();
  sheet.replaceSync(RULE);

  const motion = typeof win.matchMedia === "function" ? win.matchMedia(REDUCED_MOTION) : null;

  const apply = () => {
    const wanted = motion === null || !motion.matches;
    const adopted = doc.adoptedStyleSheets.indexOf(sheet) !== -1;
    if (wanted === adopted) return;
    doc.adoptedStyleSheets = wanted
      ? doc.adoptedStyleSheets.concat(sheet)
      : doc.adoptedStyleSheets.filter((other) => other !== sheet);
  };

  apply();
  motion?.addEventListener?.("change", apply);
  return true;
}

// The bundle the theme loads with the page. One job so far; the rest of the
// runtime is still vendored in `crates/liyasa-theme/assets/js/`
// (`plan/rfcs/1100-reader-toolchain.md`).

install(window         );
})();
