import "@/styles/globals.css";
import { Shell } from "@/components/layout/Shell";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="h-full">
      <body>
        <Shell>{children}</Shell>
      </body>
    </html>
  );
}
