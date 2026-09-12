# Security

Principia Desk runs on your machine, holds your provider keys, and can lock the screen. Those three things are what this policy is about.

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x | yes |
| earlier (System Design Roulette) | no |

## What counts

Please report, privately, anything that lets:

- a lock hold a machine when one of the documented ways out should release it (break-glass phrase, recovery console, release token, dead man's switch), or that lets a broken build engage the lock at all;
- a provider key, an escape phrase or a Keychain item reach a place it should not (a log, an export, a lesson, a request to another host);
- the desk fetch or execute something a lesson's source allowlist should have refused;
- an exported file (lesson PDF, CSV, class file, profile export) carry data it says it does not.

Ordinary bugs go in the issue tracker.

## How to report

Email **contact@ndelucien.com** with the version, the steps, and what you observed. Please do not open a public issue for a lockout bypass or a key leak until it is fixed. You will get an acknowledgement within a few days and a fix or a plan within two weeks for anything that can hold a machine.

## What the desk does with your data

- Lessons are written by the tutor you configure. Prompts contain the topic, the curriculum brief, your learning goal, your progress on that class and the pages the desk fetched. They do not contain your keys, your escape phrase or other classes.
- Keys live in the macOS Keychain under the `principia-desk` service. Environment variables take priority when set.
- Everything else is a SQLite database under `~/Library/Application Support/com.darkmatter.principia-desk/`, with a backup before every schema upgrade.
- The desk talks to your tutor's provider, to the documentation hosts a course allows, and to a search engine only if you configure one. Nothing else.
