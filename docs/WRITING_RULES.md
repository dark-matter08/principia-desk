# Writing rules

Every text the desk generates is held to one set of writing rules, and so is the bundled text it ships with. The rules are the signs that a language model wrote something, as Wikipedia's editors document them in [Signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing): the stock vocabulary, the constructions that carry no information, the openers and closers of a chat answer, the vague attributions, the rule of three, the em dash as the default clause break, emoji and decorative bold. A lesson, a course draft, a review finding, a question, a chat reply: none of them should read as if a machine wrote it.

## What is enforced, and where

Three mechanisms share one catalogue, `src-tauri/seed/prose-tells.json`.

**The contract in every request.** `prose::CONTRACT` is appended to the system prompt of every runner call the desk makes, whatever the purpose (`Generator::provider_request`). Every runner honours the system text: the CLI runners get it as an appended system prompt or merged ahead of the prompt, the API runners as the system message. The contract says what not to do and that a deterministic check reads the answer.

**The scrub on every answer.** What can be fixed mechanically is fixed as the answer comes off the wire, before anything parses it (`prose::scrub` in `run_wire_for` and the fallback path). An em dash, a spaced en dash or the JSON escape `—` becomes the punctuation a person uses: a comma between clauses, nothing at the end of a line, the existing mark when one is already there. Fenced code blocks and inline code are left alone. The same scrub runs when a course draft is saved (`custom::normalize`), so text a learner typed or imported is treated the same way.

**The gates.** What remains is read for the catalogue (`prose::objection`) and sent back with the reason:

| Text | Gate | What happens on a hit |
| --- | --- | --- |
| Lesson body | `validate_course_body` | The sections that carry a tell are rewritten one at a time (`clean_course_prose`), told exactly which expressions to replace; a rewrite that loses most of a section is not taken. |
| Quiz and exit checks | `validate_generated_quiz` | The usual one-round correction with the reason. |
| Course chat reply | `validate_chat_reply` | The usual one-round correction. |
| A class draft | `custom::validate` | An issue on the field or the topic, shown in the editor's rail and sent back to the tutor with the other validator reasons; the class cannot be published until it is clear. |
| Bundled lessons | `prose::bundled_tests` | A test fails when a bundled lesson carries a tell or an em dash. |

The interface mirrors the check (`src/lib/prose.ts`, from `src/lib/prose.generated.ts`) so the class editor says what the desk would object to as the learner types. The native result stays authoritative.

## The catalogue

A tell is either **hard**, which fails a text on one occurrence, or **soft**, which fails it once more than `soft_limit` (three) have piled up. Patterns match on word boundaries, case-insensitively, outside code; a leading `^` means the phrase has to open a sentence.

Hard tells: chat openers and closers ("Certainly!", "Great question", "I hope this helps", "Let me know if"), the model talking about itself ("as an AI", "as of my last", "knowledge cutoff"), summary paragraphs ("In conclusion", "In summary", "To sum up" at a sentence start), the constructions that carry no information ("not only", "it's not just", "it is important to note", "it is worth noting", "plays a crucial role", "cannot be overstated", "stands as a", "serves as a", "a testament to", "in today's", "in the realm of", "navigating the complexities", "ever-evolving", "game-changer", "deep dive", "dive into", "delve", "tapestry"), vague attribution ("experts say", "studies show", "it is widely known"), and emoji.

Soft tells: crucial, pivotal, vital, robust, comprehensive, leverage, utilize, foster, seamless, streamline, holistic, multifaceted, nuanced, intricate, meticulous, vibrant, groundbreaking, cutting-edge, state-of-the-art, transformative, revolutionize, empower, myriad, plethora, showcase, underscore, "landscape of", unleash, "at its core", "when it comes to", "a wide range of", "in order to", and the sentence openers Moreover, Furthermore, Additionally, Overall, Ultimately, Notably, Importantly, Essentially.

Words that are ordinary in a technical register are deliberately not in the catalogue even though they appear on the Wikipedia list as verbs of emphasis: harness (a test harness), unlock (a mutex), navigate (to a page), journey (a user journey), realm (a Kerberos realm), elevate (privileges). The rule of three, decorative bold and Title Case headings are in the contract but not in the check; they need judgement the check does not have.

To change the catalogue, edit `src-tauri/seed/prose-tells.json` and run `npm run catalog:generate`; `npm run check` fails while the generated mirror is stale.

## The bundled text

The prompts under `src-tauri/prompts/`, the bundled lessons under `src-tauri/seed/fallback_courses/`, the documentation and the interface's own copy were scrubbed of em dashes with `scripts/scrub-dashes.py`, which applies the same rules as the runtime scrub to files. Course fingerprints changed with the prompts and the reference lessons; an enrolled bundled class shows the ordinary "the curriculum has a newer version" notice once and keeps its accepted path.
