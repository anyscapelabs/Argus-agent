import { useEffect, useState } from "react";
import { motion } from "framer-motion";

const WORDS = [
  "Thinking",
  "Reasoning",
  "Pondering",
  "Deliberating",
  "Reflecting",
  "Considering",
];

const WORD_MS = 2200;

type Props = {
  /// A fixed label. The turn is not streaming anything — the words would be
  /// claiming work that is not happening.
  label?: string;
};

export default function StreamingIndicator({ label }: Props) {
  const [word, setWord] = useState(0);

  useEffect(() => {
    if (label !== undefined) return;

    const t = setInterval(() => {
      setWord((w) => (w + 1) % WORDS.length);
    }, WORD_MS);

    return () => clearInterval(t);
  }, [label]);

  const shown = label ?? `${WORDS[word]}…`;

  return (
    <motion.div
      className="flex items-center gap-2.5 py-1 font-sans"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.2 }}
    >
      <motion.span
        className="h-2.5 w-2.5 rounded-full bg-accent"
        animate={{ scale: [1, 1.45, 1], opacity: [1, 0.55, 1] }}
        transition={{ duration: 1.4, repeat: Infinity, ease: "easeInOut" }}
      />
      <motion.span
        key={shown}
        className="text-[14px] text-text-secondary"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.3 }}
      >
        {shown}
      </motion.span>
    </motion.div>
  );
}
