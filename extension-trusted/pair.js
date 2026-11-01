// The two BraveBot extensions name each other by id. The browser derives an
// unpacked extension's id from the key in its manifest, so each id here is the
// one that key gives, and a test holds them to the keys. FIRST_ID is the
// BraveBot extension, the only sender this extension answers. SECOND_ID is this
// extension's own id, which the BraveBot extension sends to.
export const FIRST_ID = "fcjamhpiedeihbbbpndcjngkjaogfgep";
export const SECOND_ID = "lmdelobjcmmjoapnmnkfpmjlfikgepbl";

// Whether a message came from the BraveBot extension. The browser has already
// refused every extension the manifest's externally_connectable does not name;
// this is the second check. The BraveBot extension asks from its service worker,
// which has no tab. A sender with a tab is a script that runs in a page, a content
// script of the BraveBot extension included, which the page's content could be
// working beside, and it is not answered. The sender's frame and address belong to
// a tab and are not read.
export function isFirst(sender) {
  return (
    typeof sender === "object" &&
    sender !== null &&
    sender.id === FIRST_ID &&
    sender.tab === undefined
  );
}
