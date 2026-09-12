// Generated from src-tauri/seed/catalog.json. Run npm run catalog:generate.
export type FocusArea = "system-design" | "javascript" | "typescript" | "frontend-architecture" | "developer-tooling" | "linux-bash" | "bash-scripting";
export type LanguageId = "german" | "italian";
export type CustomSubjectId = `custom-${string}`;
export type ClassroomSubjectId = FocusArea | LanguageId | CustomSubjectId;
export const COURSE_FINGERPRINTS: Record<FocusArea | LanguageId, string> = {
  "system-design": "56046134162b2cc5488dff0698f26b0bd1e78d251f4b3dd364e44597d8e53cbc",
  "javascript": "aab5c3fdd8900726e644f8b1b013944c86794b9ac3490292fb89815006ed71ed",
  "typescript": "4f0fc2dc58644b5e538c0741dc2a69234317af81f70ce7552af39b339224899c",
  "frontend-architecture": "ae2462fc49b365bd524515b4aef671cf85e8207b7f3888da06885197f82460b5",
  "developer-tooling": "1c85c06dd82175399af54bcd20e95e043e06b167e8855d19647b42512e238a12",
  "linux-bash": "4e0138ebf72d918e3b25d4662dba782e77cca08e72835a09392f35e91cd93d5c",
  "bash-scripting": "81a40d7e3be11eb1b595bd0139114ba6d786081abe12f1d9c612824f1bf80142",
  "german": "197b3015b2eb854de871771ecba6ae1de85581c1f0068767d1ab2dda96e99eea",
  "italian": "94114838a3e5959dc11d5315b956b9271709f1c83f1bf06205eef2c2a5fe6080"
};
