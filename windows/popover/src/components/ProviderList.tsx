import { useEffect, type ReactNode } from "react";
import MdiChevronRight from "~icons/mdi/chevron-right";
import { DragHandle } from "@/components/DragHandle";
import { ProviderIcon } from "@/components/ProviderIcon";
import { TruncatedText } from "@/components/TruncatedText";
import { SortableItem, VerticalDnd } from "@/components/dnd";
import { Switch } from "@/components/ui/switch";
import type { Card, Layout, Language, ProviderInfo, ProviderListRow } from "@/lib/types";
import { m } from "@/paraglide/messages.js";
import { useI18n } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { providerListRows, reorderProviderRows, sendCommand } from "../model.js";

interface ProviderListProps {
  cards: Card[];
  /** Why the last enable/disable failed, or "". */
  error: string;
  layout: Layout;
  onOpen: (id: string) => void;
  onReorder: (ids: string[]) => void;
  onToggle: (id: string, on: boolean) => void;
  providers: ProviderInfo[];
  /** The header's right side next to Detect (Settings' reset button). */
  headerExtra?: ReactNode;
}

const ADD_ACCOUNT_LABELS = { code: m.add_claude_code_account, desktop: m.add_claude_desktop_account, codex: m.add_codex_account };

/**
 * The one provider list (Customize and Settings → Providers share it): enabled
 * providers come first, in the dashboard's card order — dragging a row moves
 * the provider's whole cards — and the rest keep the catalog's order below.
 * The switch writes the provider's `enabled` to config.toml; a card without a
 * catalog row is a custom provider, switched in config.toml by hand.
 */
export function ProviderList({ cards, error, headerExtra, layout, onOpen, onReorder, onToggle, providers }: ProviderListProps) {
  // The host re-reads credentials only when asked, so ask whenever the list opens.
  useEffect(() => {
    sendCommand("providers");
  }, []);
  const { enabled, disabled } = providerListRows(providers, cards, layout);
  return (
    <div className="flex flex-col gap-[var(--header-card-gap)]">
      <div className="flex items-center justify-between gap-2">
        <div className="section-title">{m.providers()}</div>
        <div className="flex items-center gap-2">
          {headerExtra}
          <button type="button" className="text-[length:var(--sz-badge)] text-[var(--accent)]" onClick={() => sendCommand("detect")}>
            {m.detect_providers()}
          </button>
        </div>
      </div>
      {enabled.length + disabled.length > 0 ? (
        <div className="card-surface">
          <VerticalDnd
            items={enabled.map((row) => row.id)}
            onReorder={(ids) => onReorder(reorderProviderRows(layout.cardOrder, enabled, ids))}
            overlay={(id) => {
              const row = enabled.find((item) => item.id === id);
              if (!row) return null;
              return (
                <div className="lifted-surface">
                  <ProviderRow onOpen={onOpen} onToggle={onToggle} row={row} />
                </div>
              );
            }}
          >
            {enabled.map((row) => (
              <SortableItem key={row.id} id={row.id}>
                {({ attributes, listeners }) => (
                  <ProviderRow handle={{ attributes, listeners }} onOpen={onOpen} onToggle={onToggle} row={row} />
                )}
              </SortableItem>
            ))}
            {disabled.map((row) => (
              <ProviderRow key={row.id} off onOpen={onOpen} onToggle={onToggle} row={row} />
            ))}
          </VerticalDnd>
        </div>
      ) : null}
      <div className={cn("px-[var(--card-pad)] text-[length:var(--sz-badge)]", error ? "text-meter-red" : "text-label-2")}>
        {error || m.all_providers_hint()}
      </div>
    </div>
  );
}

interface ProviderRowProps {
  handle?: Parameters<typeof DragHandle>[0];
  /** Drawn in the disabled block: faded, no drag handle, no chevron. */
  off?: boolean;
  onOpen: (id: string) => void;
  onToggle: (id: string, on: boolean) => void;
  row: ProviderListRow;
}

/**
 * One provider row: grip, mark, name + subtitle, switch, chevron. The
 * subtitle reads "N metrics" for a single card and the catalog's status
 * otherwise; two or more cards list them as indented sub-rows under the name.
 */
function ProviderRow({ row, handle, off = false, onOpen, onToggle }: ProviderRowProps) {
  const { language } = useI18n();
  const single = !off && row.cards.length === 1 ? row.cards[0] : null;
  const statusText = row.state === "off" ? m.provider_off()
    : row.state === "off-ready" ? m.provider_off_ready()
      : row.state === "ready" ? m.provider_ready()
        : row.state === "needs" ? row.signIn
          : "";
  const subtitle = single ? metricsText(single.metrics, language) : statusText;
  const subtitleTone = single || row.state !== "needs" ? "text-label-2" : "text-meter-yellow";
  const action = row.state === "needs" && row.action
    ? row.action === "terminal" ? m.provider_sign_in()
      : row.action === "app" ? m.provider_open_app()
        : m.provider_add_key()
    : "";
  const identity = (
    <>
      <ProviderIcon className="text-label-2" size="var(--row-icon-box)" slug={row.icon} title={row.name} />
      <span className="flex min-w-0 flex-col">
        <TruncatedText className="text-[length:var(--sz-header)] font-semibold">{row.name}</TruncatedText>
        {subtitle ? (
          <span className={cn("text-[length:var(--sz-badge)] break-words", subtitleTone)}>{subtitle}</span>
        ) : null}
      </span>
    </>
  );
  return (
    <div data-provider-id={row.id} className={cn("flex flex-col", off && "opacity-55")}>
      <div className="flex items-center gap-[var(--row-gap)] px-[var(--card-pad)] py-[var(--pad-control)]">
        {handle ? (
          <DragHandle attributes={handle?.attributes} listeners={handle?.listeners} />
        ) : (
          // Rows without a drag target (off block, overlay) get the same 16×22
          // footprint as the grip, so the name line stays aligned — no handle.
          <span className="grip" aria-hidden />
        )}
        {single ? (
          <button type="button" className="plain-btn hover-fill flex min-w-0 flex-1 items-center gap-[var(--row-gap)]" onClick={() => onOpen(single.id)}>
            {identity}
          </button>
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-[var(--row-gap)]">{identity}</div>
        )}
        {action ? (
          <button type="button" className="action-btn" onClick={() => sendCommand("sign-in-provider", { id: row.id })}>
            {action}
          </button>
        ) : null}
        {row.switchable ? (
          <Switch
            checked={row.enabled}
            aria-label={`${m.provider_enable()} ${row.name}`}
            onCheckedChange={(on) => onToggle(row.id, on === true)}
          />
        ) : null}
        {single ? (
          <button type="button" aria-label={`${m.open()} ${single.title}`} className="plain-btn hover-icon grid size-[var(--row-icon-box)] place-items-center" onClick={() => onOpen(single.id)}>
            <MdiChevronRight className="size-[var(--icon-row)] text-label-3" />
          </button>
        ) : null}
      </div>
      {row.enabled && row.addAccounts.length > 0 ? (
        <div className="flex flex-wrap gap-x-[var(--row-gap)] px-[var(--card-pad)] pb-[var(--pad-control)] pl-[calc(var(--card-pad)+var(--row-icon-box)+var(--row-gap))]">
          {row.addAccounts.map((kind) => (
            <button key={kind} type="button" className="text-[length:var(--sz-badge)] text-[var(--accent)]" onClick={() => sendCommand("add-account", { kind })}>
              {ADD_ACCOUNT_LABELS[kind]()}
            </button>
          ))}
        </div>
      ) : null}
      {!off && row.cards.length > 1 ? (
        <div className="flex flex-col">
          {row.cards.map((card) => (
            <div key={card.id} data-card-id={card.id} className="flex items-center gap-[var(--row-gap)] px-[var(--card-pad)] py-[var(--pad-control)] pl-[calc(var(--card-pad)+var(--row-icon-box)+var(--row-gap))]">
              <button type="button" className="plain-btn hover-fill flex min-w-0 flex-1 items-center gap-[var(--row-gap)]" onClick={() => onOpen(card.id)}>
                <span className="flex min-w-0 flex-col">
                  <TruncatedText className="text-[length:var(--sz-body)]">{card.title}</TruncatedText>
                  <span className="text-[length:var(--sz-badge)] text-label-2">{metricsText(card.metrics, language)}</span>
                </span>
              </button>
              <button type="button" aria-label={`${m.open()} ${card.title}`} className="plain-btn hover-icon grid size-[var(--row-icon-box)] place-items-center" onClick={() => onOpen(card.id)}>
                <MdiChevronRight className="size-[var(--icon-row)] text-label-3" />
              </button>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function metricsText(count: number, language: Language) {
  return `${count} ${count === 1 ? m.metric_singular({}, { locale: language }) : m.metric_plural({}, { locale: language })}`;
}
