# letters-ai

The optional **AI tools** of Letters: summarise a document, list its key
points, answer a question with quotes that are checked against the page,
and "jump to where it talks about X".

They are **off by default**. Nothing in this crate contacts any server until
the person turns the tools on in Settings *and* fills in a provider
themselves. There are no built-in addresses, model names or keys.

## What leaves this computer, and when

Every task runs in two steps. First `plan` works out exactly what would be
sent and returns a `Disclosure` with a plain sentence for the screen. Only
after the person agrees is it run.

| Provider | Where text goes | What the disclosure says |
|---|---|---|
| none | nowhere ("find where" only) | nothing is sent |
| llama.cpp server on this computer | stays on this computer | "This stays on this computer." |
| llama.cpp server on another computer | that computer | "This will send … to 192.168.1.20." |
| online service with your key | that service | "This will send the text of pages 3–7 (about 2,400 words) to api.example.com." |

What each task sends:

| Task | Sent |
|---|---|
| Summarise, Key Points | the text of the chosen pages (all of it) |
| Ask, without embeddings | your question and only the few passages that match it, chosen on this computer. If nothing matches, nothing is sent and the answer is "The document doesn’t say." |
| Ask, with embeddings | your question and the text of the chosen pages (it all has to be embedded to be searched by meaning) |
| Find Where, without embeddings | nothing: it is worked out on this computer |
| Find Where, with embeddings | your search words and the text of the chosen pages |

How this is kept true:

- **"On this computer" is checked, not guessed.** The server address is
  looked up, and only when *every* address it resolves to is this computer
  (127.x.x.x or ::1) does the text count as staying here. Requests then go to
  the exact address that was checked.
- **Sending needs the person's yes in code, not just in the interface.**
  Providers refuse to send without a `Permit`. Only `AiTools::run` can make
  one, and only from an `Acknowledgement` of the very disclosure the plan
  produced (each plan has its own id, and is used up when run). Requests can
  only be built inside the crate, so no other code can slip text past this.
- **No redirects.** A server that says "go over there instead" is refused,
  so text can't end up somewhere the disclosure didn't name.
- **No proxy for local servers.** A computer-wide proxy setting is ignored
  for them, so "local" text isn't carried off the machine.
- **Plain http only on this computer.** An online service must use https.
- **The key** is held in `ApiKey`, which prints as `***` everywhere, is never
  put in an error message (not even when the server quotes it back), and is
  not part of the settings. The settings hold only the key's name; the
  desktop app stores the key itself in the system's password store through
  the `KeyStore` trait.
- **Documents are data.** Document text only goes inside a marked block in
  the user message, never in the instructions, and the instructions tell the
  model to treat anything in the block as text to read, not orders to follow.

## Setting up a provider

### A model on this computer (llama.cpp)

You need two files you already have: the llama.cpp server program, and a
model file for it. Letters never downloads either.

**Letters starts it for you**

1. In Settings, turn on AI tools and choose **Model on this computer**.
2. Choose the llama.cpp server program, for example
   `C:\Tools\llama\server.exe`.
3. Choose the model file, for example `C:\Models\my-local-model.gguf`.
4. Letters starts the program on a free port on 127.0.0.1, waits until the
   model has loaded, and stops it again when you quit.

**You start it yourself**

1. Start the server on this computer, for example on port 8080.
2. In Settings, choose **Model on this computer** and type the address,
   `http://127.0.0.1:8080`.
3. If you started it with embeddings turned on, tick **Embeddings** for
   better "find where".

### An online service with your own key

You need a service that speaks the common chat-completions format, an
account with it, and a key.

1. In Settings, turn on AI tools and choose **Online service**.
2. Type the service's address, for example `https://api.example.com/v1`.
   Letters sends requests to that address followed by `/chat/completions`.
3. Type the model name exactly as the service spells it, for example
   `my-remote-model`.
4. Paste your key. It is saved in this computer's password store, not in a
   file.
5. Optionally type an embedding model, for example `my-embedding-model`.
   Leave it empty to keep searching on this computer, which sends less.

Before anything is sent, Letters shows what will go and where, and waits
for you to agree.

## For the shell: the calls to make

```rust
use letters_ai::*;

// once, when settings change. refuses when off or not set up
let tools = AiTools::from_settings(&settings, &key_store)?;

// for each request: plan, show, confirm, run
let plan = tools.plan(&document, TaskRequest::Ask { question, pages: None })?;
let disclosure = plan.disclosure();          // serialisable for the ui
if disclosure.needs_confirmation() {
    // show disclosure.sentence() and disclosure.preview, wait for "Send"
}
let ack = disclosure.acknowledge();          // only once the person agreed
let output = tools.run(plan, Some(ack), &mut |progress| { /* to the ui */ }, &cancel)?;

// "find where" with no provider at all
let places = find_where_offline(&document, "photosynthesis", None, 10)?;

// starting a llama.cpp server the person pointed to
let server = ServerProcess::start(&LaunchOptions::new(program, model), &cancel)?;
let config = server.config();                // a LocalServerConfig for Settings
```

`Document` is pages of paragraphs, each with its text and rectangle, as the
text extraction crate gives them. Every result (summary pages, key point
pages, citations, found passages) refers back to page numbers and
rectangles, so the viewer can jump to and highlight them.

Every error is an `AiError` whose message is a sentence for the person.

## Long documents

- Text is cut into paragraphs ("pieces") and those into chunks of about 300
  tokens with one paragraph of overlap. Chunks never cross a page.
- Token counts are guessed at four characters per token, and corrected from
  the counts the server reports.
- Summaries and key points of documents too long for one request are made
  map-reduce style: notes on each part, then notes combined (in more rounds
  if needed), then the final answer.
- Questions send only the best few chunks that fit.
- Progress is reported step by step, the final answer streams as it is
  written, and Stop is checked between requests and between streamed pieces.

## Tests

`cargo test -p letters-ai`. Nothing contacts the internet: providers are
tested against a tiny http server on 127.0.0.1 inside the test, and the
process manager against a stand-in program (`tests/support/stub_server.rs`).
