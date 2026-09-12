import { z } from "zod";

// The shape of everything on the page. Content is defined as a schema here and
// nowhere else; no copy lives in a .astro file. Kept generic on purpose, so the
// site can be fed by a CMS later with a change to one module.

export const linkSchema = z.object({
  label: z.string(),
  href: z.string(),
  primary: z.boolean().default(false),
});

export const figureSchema = z.object({
  src: z.string(),
  alt: z.string(),
  caption: z.string(),
});

export const sectionSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("hero"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    deck: z.string(),
    links: z.array(linkSchema),
    facts: z.array(z.string()).default([]),
    figure: figureSchema,
  }),
  z.object({
    kind: z.literal("steps"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    lede: z.string().optional(),
    items: z.array(z.object({ title: z.string(), body: z.string() })),
  }),
  z.object({
    kind: z.literal("features"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    lede: z.string().optional(),
    items: z.array(z.object({ title: z.string(), body: z.string(), icon: z.string().optional() })),
  }),
  z.object({
    kind: z.literal("classes"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    lede: z.string().optional(),
    items: z.array(z.object({ code: z.string(), name: z.string(), field: z.string() })),
    note: z.string().optional(),
  }),
  z.object({
    kind: z.literal("showcase"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    body: z.array(z.string()),
    figure: figureSchema,
    flip: z.boolean().default(false),
    links: z.array(linkSchema).default([]),
  }),
  z.object({
    kind: z.literal("trust"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    lede: z.string(),
    levels: z.array(z.object({ name: z.string(), body: z.string() })),
    ways: z.array(z.object({ title: z.string(), body: z.string() })),
    note: z.string().optional(),
  }),
  z.object({
    kind: z.literal("install"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    lede: z.string().optional(),
    requirements: z.array(z.string()),
    steps: z.array(z.object({ title: z.string(), body: z.string(), code: z.string().optional() })),
    links: z.array(linkSchema).default([]),
  }),
  z.object({
    kind: z.literal("faq"),
    key: z.string(),
    kicker: z.string(),
    title: z.string(),
    items: z.array(z.object({ q: z.string(), a: z.string() })),
  }),
  z.object({
    kind: z.literal("cta"),
    key: z.string(),
    title: z.string(),
    body: z.string(),
    links: z.array(linkSchema),
  }),
]);

export const siteSchema = z.object({
  slug: z.string(),
  name: z.string(),
  tagline: z.string(),
  description: z.string(),
  url: z.string().url(),
  repo: z.string().url(),
  releases: z.string().url(),
  version: z.string(),
  nav: z.array(linkSchema),
  sections: z.array(sectionSchema),
  footer: z.object({ note: z.string(), links: z.array(linkSchema) }),
});

export type Site = z.infer<typeof siteSchema>;
export type Section = z.infer<typeof sectionSchema>;
export type Figure = z.infer<typeof figureSchema>;
