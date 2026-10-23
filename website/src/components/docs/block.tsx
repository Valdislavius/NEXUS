import Link from 'next/link';
import type { DocBlock } from '@/content/docs';
export function DocumentationBlock({ block }: { block: DocBlock }) {
  switch (block.type) {
    case 'paragraph': return <p>{block.text}</p>;
    case 'list': return <ul>{block.items.map(item => <li key={item}>{item}</li>)}</ul>;
    case 'steps': return <ol>{block.items.map(item => <li key={item}>{item}</li>)}</ol>;
    case 'notice': return <aside className="notice"><h3>{block.title}</h3><p>{block.text}</p></aside>;
    case 'table': return <div className="table-scroll"><table><caption className="sr-only">{block.caption}</caption><thead><tr>{block.headers.map(header => <th scope="col" key={header}>{header}</th>)}</tr></thead><tbody>{block.rows.map(row => <tr key={row[0]}>{row.map((cell, index) => <td key={index}>{cell}</td>)}</tr>)}</tbody></table></div>;
    case 'links': return <div className="doc-links">{block.items.map(link => link.href.startsWith('/') ? <Link className="text-link" key={link.href} href={link.href}>{link.label}</Link> : <a className="text-link" key={link.href} href={link.href}>{link.label}</a>)}</div>;
    case 'faq': return <div>{block.items.map(item => <details key={item.question}><summary>{item.question}</summary><p>{item.answer}</p></details>)}</div>;
  }
}
