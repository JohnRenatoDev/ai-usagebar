import { useEffect, useState } from "react";
import MdiEye from "~icons/mdi/eye";
import MdiEyeOff from "~icons/mdi/eye-off";
import MdiLoading from "~icons/mdi/loading";
import MdiLockOutline from "~icons/mdi/lock-outline";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogTitle } from "@/components/ui/dialog";
import type { KeyCheck, Language } from "@/lib/types";
import { cn } from "@/lib/utils";
import { useI18n } from "@/lib/i18n";
import { keyCheckError, keyDialogPhase, sendCommand } from "../model.js";
import { m } from "@/paraglide/messages.js";

/** The catalog row that opens the dialog, with its credential's label and note. */
export interface KeyDialogProvider {
  id: string;
  keyLabel: string;
  keyNote: string;
  name: string;
}

// The next send's number. It starts on this clock and grows by one per send,
// so a verdict is matched to its send by number alone: the host echoes back
// the `seq` it was asked with, and never has to agree with this clock.
let nextKeySeq = Date.now();

interface ApiKeyDialogProps {
  /** The provider whose key is being set; null closes the dialog. */
  provider: KeyDialogProvider | null;
  /** The host's last verdict, from the payload's `key_check`. */
  keyCheck: KeyCheck | null;
  nowMs: number;
  onClose: () => void;
}

/**
 * Set an API key and let the provider judge it. Save writes the key through
 * the host's settings writer and sends it for a check of this provider alone;
 * the host keeps the key only if the provider accepts it, and answers with the
 * verdict under the send's `seq`. The draft never stays in state after a send:
 * the host owns the value, and this dialog only ever names it on the way out.
 */
export function ApiKeyDialog({ provider, ...rest }: ApiKeyDialogProps) {
  // Mounted per opening, so a verdict from an earlier opening (the last send's
  // `ok`) can never close the next one.
  return provider ? <OpenApiKeyDialog provider={provider} {...rest} /> : null;
}

function OpenApiKeyDialog({ provider, keyCheck, nowMs, onClose }: ApiKeyDialogProps & { provider: KeyDialogProvider }) {
  const { language } = useI18n();
  const [key, setKey] = useState("");
  const [pending, setPending] = useState<{ seq: number | null; sentAtMs: number }>({ seq: null, sentAtMs: 0 });
  const [revealed, setRevealed] = useState(false);
  const phase = keyDialogPhase(keyCheck, pending.seq, pending.sentAtMs, nowMs);

  // An accepted key closes the dialog; the verdict that says so is the host's.
  useEffect(() => {
    if (phase === "ok") onClose();
  }, [phase, onClose]);

  function save() {
    if (key === "" || phase === "checking") return;
    const seq = nextKeySeq++;
    setPending({ seq, sentAtMs: Date.now() });
    setKey("");
    sendCommand("set-api-key", { id: provider.id, key, seq });
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent>
        <ApiKeyDialogView
          checking={phase === "checking"}
          error={phase === "failed" && keyCheck ? keyCheck.error : ""}
          keyDraft={key}
          language={language}
          noAnswer={phase === "no-answer"}
          provider={provider}
          revealed={revealed}
          onKeyChange={setKey}
          onRevealToggle={() => setRevealed((current) => !current)}
          onCancel={onClose}
          onSave={save}
        />
      </DialogContent>
    </Dialog>
  );
}

interface ApiKeyDialogViewProps {
  /** The spinner phase: the field and Save wait for the host's verdict. */
  checking: boolean;
  /** The refused key's error, already cut by the host ("" outside `failed`). */
  error: string;
  keyDraft: string;
  language: Language;
  /** The send has not answered within the limit. */
  noAnswer: boolean;
  provider: KeyDialogProvider;
  revealed: boolean;
  onKeyChange: (value: string) => void;
  onRevealToggle: () => void;
  onCancel: () => void;
  onSave: () => void;
}

/**
 * The dialog's content, without the portal, so the contract's SSR tests can
 * render the real markup — Radix keeps portaled content out of static
 * markup, the same way it does the menus.
 */
export function ApiKeyDialogView({
  checking,
  error,
  keyDraft,
  language,
  noAnswer,
  provider,
  revealed,
  onKeyChange,
  onRevealToggle,
  onCancel,
  onSave,
}: ApiKeyDialogViewProps) {
  const locale = { locale: language };
  const saveDisabled = checking || keyDraft === "";
  return (
    <form
      className="flex flex-col gap-[var(--gap-stack)]"
      onSubmit={(event) => {
        // Enter in the field saves, like the Save button.
        event.preventDefault();
        onSave();
      }}
    >
      <div className="flex items-center gap-[var(--gap-item)]">
        <span
          className={cn(
            "grid shrink-0 place-items-center [&_svg]:size-[var(--icon-row)]",
            checking || noAnswer ? "text-[var(--accent)]" : "text-label-2",
          )}
        >
          {checking ? <MdiLoading className="animate-spin" /> : <MdiLockOutline />}
        </span>
        <DialogTitle>{m.api_key_dialog_title({ provider: provider.name, label: provider.keyLabel }, locale)}</DialogTitle>
      </div>
      {provider.keyNote ? <DialogDescription>{provider.keyNote}</DialogDescription> : null}
      {checking ? (
        <p className="m-0 text-[length:var(--sz-support)] leading-[var(--leading-note)] text-[var(--accent)]">
          {m.api_key_checking({ provider: provider.name }, locale)}
        </p>
      ) : null}
      {noAnswer ? (
        <p className="m-0 break-words text-[length:var(--sz-support)] leading-[var(--leading-note)] text-meter-red">
          {m.api_key_no_answer({ provider: provider.name }, locale)}
        </p>
      ) : null}
      {error ? (
        <p className="m-0 break-words text-[length:var(--sz-support)] leading-[var(--leading-note)] text-meter-red">
          {keyCheckError(error, provider.name, language)}
        </p>
      ) : null}
      <div className="flex items-center gap-[var(--gap-controls)]">
        <input
          type={revealed ? "text" : "password"}
          value={keyDraft}
          disabled={checking}
          autoComplete="off"
          spellCheck={false}
          autoCapitalize="off"
          placeholder={provider.keyLabel}
          aria-label={provider.keyLabel}
          className="min-w-0 flex-1 rounded-[var(--control-radius)] bg-[var(--field-fill)] px-2 py-1 text-[length:var(--sz-support)] outline-hidden [box-shadow:0_0_0_1px_var(--field-ring)] disabled:opacity-60"
          onChange={(event) => onKeyChange(event.target.value)}
        />
        <button
          type="button"
          className="action-btn"
          disabled={checking}
          aria-label={revealed ? m.hide_key(locale) : m.show_key(locale)}
          onClick={onRevealToggle}
        >
          {revealed ? <MdiEyeOff /> : <MdiEye />}
        </button>
      </div>
      <div className="flex items-center gap-[var(--gap-item)] text-label-2">
        <MdiLockOutline className="size-[var(--icon-note)] shrink-0" />
        <span className="text-[length:var(--sz-badge)]">{m.api_key_stored_note(locale)}</span>
      </div>
      <DialogFooter>
        <button type="button" className="action-btn" onClick={onCancel}>
          {m.cancel(locale)}
        </button>
        <button type="submit" className="action-btn" data-variant="primary" disabled={saveDisabled}>
          {m.api_key_save(locale)}
        </button>
      </DialogFooter>
    </form>
  );
}
