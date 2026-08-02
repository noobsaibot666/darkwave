import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  // docs.alan-design.com is now a multi-product hub — Darkwave's docs live
  // under /darkwave/, with a hand-written homepage at the root linking out
  // to each product's own docs site.
  site: "https://docs.alan-design.com",
  base: "/darkwave",
  integrations: [
    starlight({
      title: "Darkwave",
      sidebar: [
        {
          label: "Product",
          autogenerate: { directory: "product" }
        },
        {
          label: "Technical",
          autogenerate: { directory: "technical" }
        },
        {
          label: "User Guide",
          autogenerate: { directory: "user-guide" }
        },
        {
          label: "Legal",
          autogenerate: { directory: "legal" }
        },
        {
          label: "NAS",
          autogenerate: { directory: "nas" }
        },
        {
          label: "Development",
          autogenerate: { directory: "development" }
        },
        {
          label: "Design",
          autogenerate: { directory: "design" }
        }
      ]
    })
  ]
});
