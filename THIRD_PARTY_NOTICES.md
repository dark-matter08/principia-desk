# Remote Ledger agent operation

Principia Desk adapts Remote Ledger's three runner setup routes, provider and model
selection, API adapters, Ollama setup/model shelf/download management, explicit
fallback, usage contracts and process operation. Reference commit:
`19997d3d253e48c93911364b35ef619b835e50a2`; source files under `app/llm/`,
`app/components/RunnerChoice.tsx`, `OllamaSetup.tsx`, `app/ollama.ts`,
`app/services/ollama.server.ts` and `app/routes/api-ollama.tsx`.

The native Rust integration, learning prompts, source policy, SQLite storage and
Principia visual system are adapted for this desktop application. The source
license follows.

MIT License

Copyright (c) 2026 The Remote Ledger contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
# Bundled fonts

Fraunces, Inter and JetBrains Mono are distributed under the SIL Open Font
License 1.1. Their original copyright notices and license texts are included in
`static/fonts/*-OFL.txt`, alongside the unmodified WOFF2 subsets. These fonts are
bundled so the desktop interface does not need an external font stylesheet at
startup. Source packages are `@fontsource-variable/fraunces`,
`@fontsource-variable/inter` and `@fontsource-variable/jetbrains-mono`, version
5.3.0. Package integrity values are recorded in `static/fonts/sources.json`.
