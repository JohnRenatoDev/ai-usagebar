import { createContext, useContext } from "react";

/**
 * The fixes the popover makes itself instead of sending the user elsewhere:
 * the key dialog for a key-only provider, and the Customize screen where a
 * provider is switched on. Provided by the App, used by the provider list and
 * by a card's error row.
 */
export interface PopoverActions {
  /** Open the key dialog for `providerId`; focus goes back to `trigger` when it closes. */
  addKey: (providerId: string, trigger: HTMLElement | null) => void;
  openCustomize: () => void;
}

const PopoverActionsContext = createContext<PopoverActions>({
  addKey: () => {},
  openCustomize: () => {},
});

export const PopoverActionsProvider = PopoverActionsContext.Provider;

export function usePopoverActions(): PopoverActions {
  return useContext(PopoverActionsContext);
}
