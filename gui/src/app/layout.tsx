import "@/styles/globals.css";
import { Shell } from "@/components/layout/Shell";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="h-full" style={{ backgroundColor: "#0f1115" }}>
      <body className="h-full" style={{ backgroundColor: "#0f1115" }}>
        <Shell>{children}</Shell>
      </body>
    </html>
  );
}
