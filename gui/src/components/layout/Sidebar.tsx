"use client";
import { SidebarNav } from "./SidebarNav";
import { SidebarPowerButton } from "./SidebarPowerButton";

export function Sidebar() {
  return (
    <aside className="w-[72px] flex-none flex flex-col items-stretch border-r border-[#252934] bg-[#171a21] h-full">
      <div className="flex-1 overflow-y-auto">
        <SidebarNav />
      </div>
      <div className="border-t border-[#252934] flex justify-center py-3">
        <SidebarPowerButton />
      </div>
    </aside>
  );
}
