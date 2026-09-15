(function () {
"use strict";

// RX-11: the field metrics, measured in the page.
//
// Only the three the budget names, and no beacon: the collector keeps the
// numbers in the page and the e2e suite reads them (`e2e/vitals.spec.ts`).
// Analytics has its own event schema and its own consent story (§26.1), so
// nothing here reports anywhere.

                         
                                                
                     
                                                             
              
                                                 
                     
 

                            
                     
                     
 

const BUDGET = { lcp: 1200, cls: 0.05, inp: 100 };

/** A shift belongs to the open window while it is within these of it. */
const WINDOW_GAP = 1000;
const WINDOW_SPAN = 5000;

                        
                                                  
                     
 

                               
                                                                            
                                 
 

                               
                                            
 

function number(entry                         , key        )         {
  const value = entry[key];
  return typeof value === "number" ? value : 0;
}

function collect(win              )            {
  const state         = { lcp: null, cls: 0, inp: null };
  const Observer = win.PerformanceObserver;
  const observers                 = [];

  const watch = (type        , options                         , each                                          ) => {
    if (typeof Observer !== "function") return;
    const supported = Observer.supportedEntryTypes;
    if (Array.isArray(supported) && !supported.includes(type)) return;
    try {
      const observer = new Observer((list) => {
        for (const entry of list.getEntries()) each(entry                           );
      });
      observer.observe({ type, buffered: true, ...options });
      observers.push(observer);
    } catch {
      // An older browser that knows the name but not the option; nothing to
      // measure is better than a page that breaks measuring it.
    }
  };

  watch("largest-contentful-paint", {}, (entry) => {
    state.lcp = number(entry, "renderTime") || number(entry, "startTime");
  });

  let windowValue = 0;
  let windowStart = 0;
  let windowLast = 0;
  watch("layout-shift", {}, (entry) => {
    if (entry["hadRecentInput"] === true) return;
    const at = number(entry, "startTime");
    const value = number(entry, "value");
    if (windowValue !== 0 && at - windowLast < WINDOW_GAP && at - windowStart < WINDOW_SPAN) {
      windowValue += value;
    } else {
      windowValue = value;
      windowStart = at;
    }
    windowLast = at;
    if (windowValue > state.cls) state.cls = windowValue;
  });

  watch("event", { durationThreshold: 16 }, (entry) => {
    if (!number(entry, "interactionId")) return;
    const duration = number(entry, "duration");
    if (state.inp === null || duration > state.inp) state.inp = duration;
  });

  return {
    snapshot: () => ({ lcp: state.lcp, cls: state.cls, inp: state.inp }),
    disconnect: () => {
      for (const observer of observers) observer.disconnect();
    },
  };
}

/** The budgets a measurement misses, in the order RX-11 lists them. */
function over(vitals        , budget = BUDGET)           {
  const missed           = [];
  if (vitals.lcp !== null && vitals.lcp > budget.lcp) {
    missed.push(`LCP ${vitals.lcp} ms, over ${budget.lcp} ms`);
  }
  if (vitals.cls > budget.cls) missed.push(`CLS ${vitals.cls}, over ${budget.cls}`);
  if (vitals.inp !== null && vitals.inp > budget.inp) {
    missed.push(`INP ${vitals.inp} ms, over ${budget.inp} ms`);
  }
  return missed;
}

// The collector as a loadable script: `e2e/vitals.spec.ts` adds it before the
// page runs and reads `window.__liyasaVitals` after. It is not part of the
// bundle a reader loads.

(window                                    )["__liyasaVitals"] = collect(window         );
})();
