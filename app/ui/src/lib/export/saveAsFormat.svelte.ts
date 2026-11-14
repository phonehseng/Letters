// Save As asks for the format first, in Letters itself, then opens the save
// panel with that one type. a mac's save panel shows no format menu for a
// list of types, so Save As there could only keep the document's own
// format; asking first works the same on every system. one request at a
// time: the sheet (SaveAsFormatSheet.svelte, placed once in App.svelte)
// shows it and answers it.

export interface FormatOption {
  id: string;
  label: string;
  extension: string;
  available: boolean;
  /** what the format keeps, or why it can't be used here */
  note: string;
}

interface Request {
  options: readonly FormatOption[];
  chosen: string;
  answer: (option: FormatOption | null) => void;
}

let request = $state<Request | null>(null);

/** the request the sheet shows, if any. */
export const saveAsFormat = {
  get current(): Request | null {
    return request;
  },
};

/** asks which format to save in; null when the person cancels. `initial` is chosen to start with. */
export function chooseSaveFormat<T extends FormatOption>(options: readonly T[], initial: string): Promise<T | null> {
  // a second request (a double press) replaces the first, which is cancelled
  request?.answer(null);
  return new Promise((resolve) => {
    const usable = options.find((option) => option.id === initial && option.available) ?? options.find((option) => option.available);
    request = {
      options,
      chosen: usable?.id ?? initial,
      answer: (option) => {
        request = null;
        // the option is one of `options`, so it is a T
        resolve(option as T | null);
      },
    };
  });
}

/** what Save As offers for a pdf: the pdf itself, or the formats Export To… makes. */
export const PDF_SAVE_FORMATS: readonly FormatOption[] = [
  { id: "pdf", label: "PDF", extension: "pdf", available: true, note: "This PDF with its changes, under a new name or in another folder." },
  { id: "export", label: "Word, Excel, PowerPoint, pictures, text or a web page", extension: "", available: true, note: "Continue opens Export To…, which makes a copy in the format you choose there, with a choice of pages." },
];
