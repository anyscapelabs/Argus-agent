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

export default function StreamingIndicator() {
  const [word, setWord] = useState(0);

  useEffect(() => {
    const t = setInterval(() => {
      setWord((w) => (w + 1) % WORDS.length);
    }, WORD_MS);

    return () => clearInterval(t);
  }, []);

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
        key={WORDS[word]}
        className="text-[14px] text-text-secondary"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.3 }}
      >
        {WORDS[word]}…
      </motion.span>
    </motion.div>
  );
}
