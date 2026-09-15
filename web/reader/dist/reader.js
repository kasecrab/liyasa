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

// RX-62: `mod+shift+c` copies the page as Markdown.
//
// The action itself is the theme's — `crates/liyasa-theme/src/actions.rs`
// resolves it and `assets/js/copy.js` fetches the `.md` twin, writes the
// clipboard, and announces the result. This module only reaches the same
// button a pointer would, so the shortcut and the menu item cannot drift
// (`plan/rfcs/1103-copy-markdown-shortcut.md`).

const SELECTOR = '[data-ly-action="copy-markdown"]';

                         
               
                    
                    
                     
                   
                          
 

                      
                  
                
 

                            
                                                     
                                                                           
 

                                 
                             
 

// TODO(rfc-1103): read the chord from an `accelerator` on the action once
// `liyasa_theme::actions::Action` carries one, so the menu can print the hint.
function chord(event               )          {
  if (event.altKey === true) return false;
  if (event.shiftKey !== true) return false;
  if (event.ctrlKey !== true && event.metaKey !== true) return false;
  // Shift makes the key "C" on most layouts and "c" where it does not.
  return String(event.key ?? "").toLowerCase() === "c";
}

function bind(win                )          {
  const doc = win.document;
  if (doc.querySelector(SELECTOR) === null) return false;

  doc.addEventListener("keydown", (raw         ) => {
    const event = raw                 ;
    if (!chord(event)) return;
    // Looked up per press: the menu is re-rendered across a view transition,
    // and copy.js leaves the button hidden until it has a handler for it.
    const button = doc.querySelector(SELECTOR);
    if (button === null || button.hidden) return;
    event.preventDefault?.();
    button.click();
  });
  return true;
}

// The bundle the theme loads with the page. Two jobs so far; the rest of the
// runtime is still vendored in `crates/liyasa-theme/assets/js/`
// (`plan/rfcs/1100-reader-toolchain.md`).


install(window         );
bind(window         );
})();
