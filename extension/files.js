// What a file argument of choose_file (upload-trusted) must look like, and which
// files are never chosen. This file is copied byte for byte into the second
// extension (extension-trusted/), so it imports nothing; shared-files.json lists
// every file that is.
//
// The relay's server (crates/browser/src/uploads.rs) turns the path the planner
// wrote into the file it leads to inside the uploads directory, and sends the
// canonical path, the size and the name of that file in place of the text. Neither
// extension reads the file or looks at the disk: each checks that what it is sent
// has the form of what the relay sends and is a file that may be chosen, and the
// browser reads the file itself when the second extension asks it to. A file that
// fails the check is refused before anything is sent on.

// The largest file that is chosen, in bytes (10 MiB), and the most characters of
// a path. The relay holds a file to the same two limits. The window shows the
// path, the name and the size in one text of at most 2,000 characters, so the
// path is held to less than half of that.
export const MAX_FILE_BYTES = 10 * 1024 * 1024;
export const PATH_LIMIT = 900;

// The parts of a path that are the names of credentials, as the relay's list has
// them (crates/browser/src/uploads.rs, which a test of the relay holds equal to
// this one). A part is compared without regard to case. A part that is one of
// EXACT, or starts with one of PREFIXES, or ends with one of SUFFIXES, is the name
// of a file or of a directory that holds keys, tokens or passwords.
export const CREDENTIAL_EXACT = Object.freeze([".env", ".netrc", ".npmrc", ".pgpass", ".git-credentials", ".ssh", ".aws", ".gnupg"]);
export const CREDENTIAL_PREFIXES = Object.freeze([".env.", "id_rsa", "id_dsa", "id_ecdsa", "id_ed25519", "credentials"]);
export const CREDENTIAL_SUFFIXES = Object.freeze([".pem", ".key", ".p12", ".pfx", ".kdbx"]);

// The directory the relay's files are in, inside the directory its server was
// declared with (crates/browser/src/uploads.rs DIRECTORY).
export const UPLOADS_DIRECTORY = "uploads";

// Whether one part of a path is the name of a credential.
export function looksLikeACredential(part) {
  const name = String(part).toLowerCase();
  return (
    CREDENTIAL_EXACT.includes(name) ||
    CREDENTIAL_PREFIXES.some((prefix) => name.startsWith(prefix)) ||
    CREDENTIAL_SUFFIXES.some((suffix) => name.endsWith(suffix))
  );
}

// Whether `text` holds a character a person cannot see: a control, format, line
// separator, paragraph separator or private-use character, which the relay holds
// to the same set (uploads.rs `has_unseen`). A lone surrogate is in the set as
// well, which a Rust string cannot hold and the relay never sends.
const hasUnseen = (text) => /[\p{Cc}\p{Cf}\p{Zl}\p{Zp}\p{Co}\p{Cs}]/u.test(text);

// The parts of `parts` that name the file: the ones after the last directory
// called uploads, which are the ones inside the directory the relay chooses from,
// and all of them for a path with no such directory. The directories above that
// one are the person's own, and a name there is no credential of a file.
function partsOfTheFile(parts) {
  return parts.slice(parts.lastIndexOf(UPLOADS_DIRECTORY) + 1);
}

// What is wrong with the file of a call, as the word of the first thing that is,
// or null: "path" when it is not an absolute path of at most PATH_LIMIT
// characters, as text that holds nothing a person cannot see and no part that is
// empty, "." or ".."; "size" when it is not a whole number of bytes from 0 to
// MAX_FILE_BYTES; "file_name" when it is not the last part of the path;
// "credential" when a part of the path below the last uploads directory, or any
// part of a path with none, is the name of a credential.
export function problemWithFile({ path, size, file_name: fileName }) {
  if (typeof path !== "string" || [...path].length > PATH_LIMIT) {
    return "path";
  }
  const parts = path.split("/");
  if (
    !path.startsWith("/") ||
    hasUnseen(path) ||
    parts.slice(1).some((part) => part === "" || part === "." || part === "..")
  ) {
    return "path";
  }
  if (!Number.isSafeInteger(size) || size < 0 || size > MAX_FILE_BYTES) {
    return "size";
  }
  if (typeof fileName !== "string" || fileName !== parts.at(-1)) {
    return "file_name";
  }
  return partsOfTheFile(parts).some(looksLikeACredential) ? "credential" : null;
}

// A size in words: the bytes, and the same in KiB or MiB from 1 KiB up.
export function sizeWords(size) {
  if (size < 1024) {
    return `${size} bytes`;
  }
  const unit = size < 1024 * 1024 ? ["KiB", 1024] : ["MiB", 1024 * 1024];
  return `${size} bytes (${(size / unit[1]).toFixed(1)} ${unit[0]})`;
}

// The file as the confirmation window shows it, in one text: the name, the size
// and the full path.
export function fileWords({ path, size, file_name: fileName }) {
  return `${fileName}, ${sizeWords(size)}, ${path}`;
}
