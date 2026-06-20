import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Oxide — Live Dashboard",
  description: "Monitoreo en tiempo real del load balancer Oxide",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="es">
      <body>{children}</body>
    </html>
  );
}
