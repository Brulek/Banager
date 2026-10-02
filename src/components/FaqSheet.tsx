import { useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { FAQ_ITEMS, useFaqSheet, type FaqId, type FaqItem, type FaqView } from "../lib/faq";
import { useUiStore } from "../store/ui";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { GROUP, GROUP_ROW, GROUP_TITLE } from "./ui/group";

/** Each question's words, spelled out. A `Record`, so a question added without them fails `tsc`. */
const QUESTION_KEYS: Record<FaqId, { question: string; answer: string }> = {
  notFound: { question: "faq.questions.notFound.question", answer: "faq.questions.notFound.answer" },
  cantUpdate: { question: "faq.questions.cantUpdate.question", answer: "faq.questions.cantUpdate.answer" },
  twins: { question: "faq.questions.twins.question", answer: "faq.questions.twins.answer" },
  password: { question: "faq.questions.password.question", answer: "faq.questions.password.answer" },
  leftBehind: { question: "faq.questions.leftBehind.question", answer: "faq.questions.leftBehind.answer" },
  changesMac: { question: "faq.questions.changesMac.question", answer: "faq.questions.changesMac.answer" },
  otherPrograms: { question: "faq.questions.otherPrograms.question", answer: "faq.questions.otherPrograms.answer" },
  sizes: { question: "faq.questions.sizes.question", answer: "faq.questions.sizes.answer" },
  majorUpdate: { question: "faq.questions.majorUpdate.question", answer: "faq.questions.majorUpdate.answer" },
  autoCheck: { question: "faq.questions.autoCheck.question", answer: "faq.questions.autoCheck.answer" },
};

/**
 * What a question's button says, by where it goes: "Show in Updates",
 * "Show in Settings" -- never a bare "Show" (walk-3 W3-5); 「查看」 in
 * Chinese. A `Record`, so a kind of view added without one fails `tsc`.
 */
const VIEW_LABEL_KEYS: Record<FaqView["kind"], string> = {
  installed: "families.viewIn.installed",
  installedBySize: "families.viewIn.installed",
  updates: "families.viewIn.updates",
  unknown: "families.viewIn.unknown",
  settings: "families.viewIn.settings",
};

/**
 * One question: the question as the group's title, over a group of one row
 * -- the answer, in the label colour, and on its right, where Banager has a
 * place to act on it, a grey 查看 that closes the sheet and goes there, as
 * the setup check's lines do. VoiceOver hears the 查看 with its question.
 */
function Question({ item, onView }: { item: FaqItem; onView: (view: FaqView) => void }) {
  const { t } = useTranslation();
  const titleId = useId();
  const keys = QUESTION_KEYS[item.id];
  const question = t(keys.question);
  const view = item.view;
  const action = view === null ? "" : t(VIEW_LABEL_KEYS[view.kind]);
  return (
    <section aria-labelledby={titleId} data-faq={item.id} className="mt-4">
      <h3 id={titleId} className={GROUP_TITLE}>
        {question}
      </h3>
      <div className={GROUP}>
        <div className={GROUP_ROW}>
          <p className="min-w-0 flex-1 break-words py-0.5 text-body text-foreground">{t(keys.answer)}</p>
          {view !== null ? (
            <button
              type="button"
              onClick={() => onView(view)}
              aria-label={t("faq.viewLabel", { action, question })}
              className={`${BUTTON.regular.grey} shrink-0`}
            >
              {action}
            </button>
          ) : null}
        </div>
      </div>
    </section>
  );
}

/**
 * 「常见问题」, the sheet Help's item of that name opens (`useFaqSheet`):
 * the questions people ask most (`FAQ_ITEMS`, src/lib/faq.ts), each a
 * heading over its answer. It changes nothing; a question's 查看 closes it
 * and opens the list the answer points to -- the Installed page with a
 * 「显示」 choice picked, or sorted by size; the Updates page; Other
 * Programs; Settings. Done, the default button, closes it, and so do
 * Escape and a click beside it.
 */
export function FaqSheet() {
  const { t } = useTranslation();
  const open = useFaqSheet((s) => s.open);
  const close = () => useFaqSheet.setState({ open: false });
  const doneRef = useRef<HTMLButtonElement>(null);
  const openInstalled = useUiStore((s) => s.openInstalled);
  const setInstalledShow = useUiStore((s) => s.setInstalledShow);
  const setInstalledSort = useUiStore((s) => s.setInstalledSort);
  const openPage = useUiStore((s) => s.openPage);

  const onView = (view: FaqView) => {
    close();
    switch (view.kind) {
      case "installed":
        openInstalled(null);
        setInstalledShow(view.show);
        return;
      case "installedBySize":
        openInstalled(null);
        setInstalledSort("size");
        return;
      case "updates":
        openPage("updates");
        return;
      case "unknown":
        openPage("unknown");
        return;
      case "settings":
        openPage("settings");
        return;
      default: {
        const unhandled: never = view;
        return unhandled;
      }
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => useFaqSheet.setState({ open: next })}
      title={t("faq.title")}
      width="several"
      initialFocus={doneRef}
      footer={
        <button ref={doneRef} type="button" onClick={close} className={BUTTON.large.default}>
          {t("common.done")}
        </button>
      }
    >
      {open ? FAQ_ITEMS.map((item) => <Question key={item.id} item={item} onView={onView} />) : null}
    </Dialog>
  );
}
