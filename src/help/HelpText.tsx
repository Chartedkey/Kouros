// Renders the tiny markup of the help texts: paragraphs, «- » lists, «1. » numbered lists, **bold**.
import { Fragment, type ReactNode } from 'react';

function inline(t: string, mark?: string): ReactNode[] {
  const out: ReactNode[] = [];
  t.split(/(\*\*[^*]+\*\*)/g).forEach((part, i) => {
    const bold = part.startsWith('**') && part.endsWith('**');
    const text = bold ? part.slice(2, -2) : part;
    const pieces: ReactNode[] = [];
    if (mark && mark.length >= 2) {
      const re = new RegExp(`(${mark.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi');
      text.split(re).forEach((p, j) => pieces.push(p.toLowerCase() === mark.toLowerCase() ? <mark key={j}>{p}</mark> : <Fragment key={j}>{p}</Fragment>));
    } else pieces.push(text);
    out.push(bold ? <b key={i}>{pieces}</b> : <Fragment key={i}>{pieces}</Fragment>);
  });
  return out;
}

export function HelpText({ text, mark }: { text: string; mark?: string }) {
  const blocks = text.trim().split(/\n\s*\n/);
  return (
    <div className="help-text">
      {blocks.map((b, i) => {
        const lines = b.split('\n').map((l) => l.trim()).filter(Boolean);
        if (lines.every((l) => l.startsWith('- '))) return <ul key={i}>{lines.map((l, j) => <li key={j}>{inline(l.slice(2), mark)}</li>)}</ul>;
        if (lines.every((l) => /^\d+\.\s/.test(l))) return <ol key={i}>{lines.map((l, j) => <li key={j}>{inline(l.replace(/^\d+\.\s/, ''), mark)}</li>)}</ol>;
        // A paragraph followed by a list: «Text:\n- a\n- b».
        const firstList = lines.findIndex((l) => l.startsWith('- '));
        if (firstList > 0 && lines.slice(firstList).every((l) => l.startsWith('- '))) {
          return (
            <Fragment key={i}>
              <p>{inline(lines.slice(0, firstList).join(' '), mark)}</p>
              <ul>{lines.slice(firstList).map((l, j) => <li key={j}>{inline(l.slice(2), mark)}</li>)}</ul>
            </Fragment>
          );
        }
        return <p key={i}>{inline(lines.join(' '), mark)}</p>;
      })}
    </div>
  );
}
