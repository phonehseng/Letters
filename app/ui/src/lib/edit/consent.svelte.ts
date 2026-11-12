// the question before the Edit tab (or Make Editable Text) changes a
// digitally signed file. a signature covers the file's exact content, so
// any change to a page makes it invalid; the engine refuses such a change
// until the person has agreed, and the question names who signed.
//
// asked once per document: the answer holds until the document closes.
// the check is asked of the shell before each change, since signing in
// Fill & Sign can add a signature at any time; it costs one quick call.

import { contentApi, type EditCheck, type EditConsent } from "./contentApi";
import { consentQuestion } from "./changeSentences";

export interface ConsentQuestion {
  message: string;
  answer: (agreed: boolean) => void;
}

export class ConsentGate {
  #documentId: () => number;
  #check: (id: number) => Promise<EditCheck>;
  #agreed = false;

  /** the question while it is showing */
  asking = $state<ConsentQuestion | null>(null);

  constructor(documentId: () => number, check: (id: number) => Promise<EditCheck> = contentApi.check) {
    this.#documentId = documentId;
    this.#check = check;
  }

  /** what every round-two change sends to the shell. */
  get consent(): EditConsent {
    return { invalidateSignatures: this.#agreed };
  }

  /** true when a change may go ahead: the file isn't signed, or the person agreed. */
  async ensure(): Promise<boolean> {
    if (this.#agreed) return true;
    let check: EditCheck;
    try {
      check = await this.#check(this.#documentId());
    } catch {
      // the engine still refuses a signed file on its own; nothing is lost
      return true;
    }
    if (check.signatures.length === 0) return true;
    const signer = check.signatures.find((signature) => signature.signer)?.signer ?? null;
    const agreed = await new Promise<boolean>((resolve) => {
      this.asking = { message: consentQuestion(check.message, signer), answer: resolve };
    });
    this.asking = null;
    this.#agreed = agreed;
    return agreed;
  }

  /** runs `change` only after `ensure` said yes; false otherwise. */
  async guarded(change: () => Promise<boolean>): Promise<boolean> {
    if (!(await this.ensure())) return false;
    return change();
  }
}
