// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

const repository = 'https://github.com/brave/bravebot';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Brave Bot',
  tagline: 'A general-purpose agent with structural resistance to indirect prompt injection',
  url: process.env.DOCS_URL || 'https://brave.github.io',
  baseUrl: process.env.DOCS_BASE_URL || '/bravebot/',
  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
  },
  favicon: 'img/favicon.png',
  organizationName: 'brave',
  projectName: 'bravebot',

  presets: [
    [
      '@docusaurus/preset-classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          routeBasePath: '/',
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl: `${repository}/edit/main/docs/website/`,
        },
        blog: false,
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: {
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: 'Brave Bot',
        logo: {
          alt: 'Brave Logo',
          src: 'img/brave128.png',
        },
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'docsSidebar',
            position: 'left',
            label: 'Docs',
          },
          {
            to: '/quickstart',
            label: 'Quickstart',
            position: 'left',
          },
          {
            to: '/reference/cli',
            label: 'CLI reference',
            position: 'left',
          },
          {
            href: repository,
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              {label: 'Overview', to: '/'},
              {label: 'Quickstart', to: '/quickstart'},
              {label: 'CLI reference', to: '/reference/cli'},
            ],
          },
          {
            title: 'Project',
            items: [
              {label: 'GitHub', href: repository},
              {label: 'Issues', href: `${repository}/issues`},
              {label: 'Specs', href: `${repository}/tree/main/docs/specs`},
            ],
          },
          {
            title: 'More',
            items: [
              {label: 'Brave', href: 'https://brave.com'},
              {label: 'Brave on GitHub', href: 'https://github.com/brave'},
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Brave Software, Inc.`,
      },
      prism: {
        additionalLanguages: ['bash', 'json', 'rust', 'toml'],
      },
    }),
};

module.exports = config;
