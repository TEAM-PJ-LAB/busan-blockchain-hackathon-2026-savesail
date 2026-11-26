"use client";
import { usePathname } from "next/navigation";

const items = [["", "개요"], ["invest/", "투자"], ["schedule/", "일정"], ["verify/", "검증"], ["settle/", "정산·배분"]];

export default function Nav() {
  const path = usePathname() ?? "/";
  const base = process.env.NEXT_PUBLIC_BASE_PATH ?? "";
  return (
    <nav aria-label="화면">
      <ul>
        {items.map(([href, label]) => {
          const full = `${base}/${href}`;
          const current = path === `/${href}` || path === full || path === `/${href}`.replace(/\/$/, "");
          return <li key={href}><a href={full} aria-current={current ? "page" : undefined}>{label}</a></li>;
        })}
      </ul>
    </nav>
  );
}
