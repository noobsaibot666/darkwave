import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
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
