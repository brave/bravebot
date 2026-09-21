// The parts of the demo that exist only to be seen.
//
// Playwright drives Electron over CDP, and CDP input does not move the macOS cursor. Every
// other driver in `scripts/` is fine with that — they assert, they do not perform — but a
// screen recording of a CDP-driven window shows menus opening and columns folding with no
// pointer anywhere on screen, which reads as a glitch rather than as a demonstration. So the
// pointer is drawn into the page and moved deliberately, and a click is marked with a ripple
// so it registers as an event somebody caused.
//
// The captions are the other half. A silent recording of an unfamiliar app does not explain
// itself, and the alternative — narrating over it live — means re-recording the audio every
// time a scene changes. A caption per beat is also a subtitle track and a script to read.
//
// All of it is appended to `<body>`, *outside* `#root`, because React owns everything inside
// `#root` and would take the overlay away with the first re-render. And all of it is
// `pointer-events: none`: an overlay that swallowed a click would leave the demo driving
// itself instead of the product.
//
// The renderer's CSP is `default-src 'none'; style-src 'self' 'unsafe-inline'`, so an
// injected `<style>` and inline styles are allowed and nothing here fetches anything. The
// pointer is an inline `<svg>` element rather than an image for the same reason.

/** Installed into the page once per launch. Everything after it goes through `window.__demo`. */
export const INSTALL = () => {
  if (window.__demo) return
  const root = document.createElement('div')
  root.id = '__demo'
  root.innerHTML = `
    <style>
      #__demo, #__demo * { pointer-events: none !important; }
      /* The real macOS pointer is hidden wherever it happens to be sitting over this window,
         because a screen recording composites whatever cursor the window asks for — and two
         pointers on screen, one of them not moving, reads as a rendering fault. The drawn one
         below is then the only pointer in the film. This is also why the demo can be left
         running without anybody keeping their hand off the trackpad. */
      html, body, body * { cursor: none !important; }
      #__demo {
        position: fixed; inset: 0; z-index: 2147483647;
        font: 500 13px/1.45 -apple-system, BlinkMacSystemFont, "SF Pro Text", system-ui, sans-serif;
      }
      #__demo-cursor {
        position: absolute; top: 0; left: 0; width: 24px; height: 24px;
        transform: translate3d(-100px, -100px, 0);
        transition: transform var(--glide, 600ms) cubic-bezier(.22, .61, .36, 1);
        filter: drop-shadow(0 2px 5px rgba(0, 0, 0, .45));
        opacity: 0;
      }
      #__demo-cursor.on { opacity: 1; }
      #__demo-ripple {
        position: absolute; top: 0; left: 0; width: 26px; height: 26px; margin: -13px 0 0 -13px;
        border-radius: 50%; opacity: 0;
        border: 2px solid rgba(120, 190, 255, .95);
        background: rgba(120, 190, 255, .22);
      }
      #__demo-ripple.tap { animation: __demo-tap 520ms cubic-bezier(.2, .7, .3, 1); }
      @keyframes __demo-tap {
        from { opacity: 1; transform: scale(.35); }
        to   { opacity: 0; transform: scale(2.4); }
      }
      #__demo-ring {
        position: absolute; border-radius: 9px; opacity: 0;
        border: 2px solid rgba(120, 190, 255, .95);
        box-shadow: 0 0 0 3px rgba(120, 190, 255, .18), 0 0 22px rgba(120, 190, 255, .35);
        transition: opacity 260ms ease, top 320ms ease, left 320ms ease,
                    width 320ms ease, height 320ms ease;
      }
      #__demo-ring.on { opacity: 1; }
      #__demo-caption {
        position: absolute; left: 50%; bottom: 46px; transform: translate(-50%, 14px);
        max-width: min(780px, 82vw); padding: 13px 22px 15px;
        border-radius: 13px; text-align: center;
        background: rgba(18, 20, 24, .93);
        border: 1px solid rgba(255, 255, 255, .12);
        box-shadow: 0 12px 40px rgba(0, 0, 0, .45);
        color: #f2f4f8; opacity: 0;
        transition: opacity 380ms ease, transform 380ms ease, top 380ms ease, bottom 380ms ease;
      }
      #__demo-caption.on { opacity: 1; transform: translate(-50%, 0); }
      /* Docked at the top instead, for when the thing being demonstrated is down at the
         bottom of the window — a menu hanging off the composer, or the composer itself. */
      #__demo-caption.high { bottom: auto; top: 62px; transform: translate(-50%, -14px); }
      #__demo-caption.high.on { transform: translate(-50%, 0); }
      #__demo-caption b {
        display: block; font-size: 11px; font-weight: 660; letter-spacing: .09em;
        text-transform: uppercase; color: #7fbcff; margin-bottom: 3px;
      }
      #__demo-caption span { display: block; font-size: 15px; line-height: 1.5; }
      #__demo-caption span:empty { display: none; }
    </style>
    <svg id="__demo-cursor" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M5 2.5 L5 19.5 L9.4 15.4 L12.2 21.6 L15.1 20.2 L12.4 14.2 L18.4 14.1 Z"
            fill="#fbfbfd" stroke="rgba(0,0,0,.65)" stroke-width="1.1" stroke-linejoin="round"/>
    </svg>
    <div id="__demo-ripple"></div>
    <div id="__demo-ring"></div>
    <div id="__demo-caption"><b></b><span></span></div>
  `
  document.body.append(root)

  const cursorEl = root.querySelector('#__demo-cursor')
  const rippleEl = root.querySelector('#__demo-ripple')
  const ringEl = root.querySelector('#__demo-ring')
  const captionEl = root.querySelector('#__demo-caption')
  const at = { x: -100, y: -100 }

  window.__demo = {
    /** Glide the pointer to a point. `ms` is the travel time, so the stage can wait exactly it. */
    cursor(x, y, ms) {
      at.x = x
      at.y = y
      cursorEl.style.setProperty('--glide', `${ms}ms`)
      cursorEl.classList.add('on')
      // The hotspot is the arrow's tip, at the top-left of the glyph, so no centring offset.
      cursorEl.style.transform = `translate3d(${x}px, ${y}px, 0)`
    },

    /** A ripple where the pointer is, so a click reads as something that happened. */
    tap() {
      rippleEl.style.transform = ''
      rippleEl.style.left = `${at.x}px`
      rippleEl.style.top = `${at.y}px`
      rippleEl.classList.remove('tap')
      void rippleEl.offsetWidth // restart the animation rather than letting it be ignored
      rippleEl.classList.add('tap')
    },

    /** A ring around a box, for pointing at something the cursor is not travelling to. */
    ring(box) {
      if (!box) return ringEl.classList.remove('on')
      ringEl.style.left = `${box.x - 5}px`
      ringEl.style.top = `${box.y - 5}px`
      ringEl.style.width = `${box.width + 10}px`
      ringEl.style.height = `${box.height + 10}px`
      ringEl.classList.add('on')
    },

    /**
     * The lower third — which moves to the *upper* third when the pointer is down in the
     * bottom of the window, because that is where the composer and the export menu are and a
     * caption sitting on top of the control being demonstrated is worse than no caption.
     * Decided from where the pointer last went rather than passed in by each scene, so a
     * caption cannot be left in the wrong place by somebody who forgot the argument.
     */
    say(title, line) {
      if (title === null) return captionEl.classList.remove('on')
      captionEl.querySelector('b').textContent = title
      captionEl.querySelector('span').textContent = line ?? ''
      captionEl.classList.toggle('high', at.y > window.innerHeight * 0.62)
      captionEl.classList.add('on')
    },
  }
}
