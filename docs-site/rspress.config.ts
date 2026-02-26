import { defineConfig } from 'rspress/config';

function normalizeBase(base: string | undefined): string {
  const raw = (base ?? '/').trim();
  if (!raw || raw === '/') return '/';
  const withLeading = raw.startsWith('/') ? raw : `/${raw}`;
  return withLeading.endsWith('/') ? withLeading : `${withLeading}/`;
}

const docsBase = normalizeBase(process.env.DOCS_BASE);

export default defineConfig({
  root: 'docs',
  base: docsBase,
  title: 'Dockrev Documentation',
  description: 'Dockrev deployment, operations, and API reference documentation.',
  lang: 'zh',
  locales: [
    {
      lang: 'zh',
      label: '简体中文',
      title: 'Dockrev 文档',
      description: 'Dockrev 的部署、运维、使用与 API 参考。'
    },
    {
      lang: 'en',
      label: 'English',
      title: 'Dockrev Docs',
      description: 'Deployment, operations, usage, and API reference for Dockrev.'
    }
  ],
  themeConfig: {
    search: true,
    nav: [
      { text: '中文', link: '/zh/' },
      { text: 'English', link: '/en/' },
      { text: 'GitHub', link: 'https://github.com/ivanli-cn/dockrev' }
    ],
    sidebar: {
      '/zh/': [
        {
          text: '总览',
          items: [
            { text: '首页', link: '/zh/' },
            { text: '快速开始', link: '/zh/quick-start' },
            { text: '部署指南', link: '/zh/deploy' },
            { text: '配置参考', link: '/zh/config' }
          ]
        },
        {
          text: '使用与运维',
          items: [
            { text: '用户使用手册', link: '/zh/user-guide' },
            { text: '运维手册', link: '/zh/operations' },
            { text: '集成指南', link: '/zh/integrations' },
            { text: '故障排查', link: '/zh/troubleshooting' },
            { text: '常见问题', link: '/zh/faq' },
            { text: '术语表', link: '/zh/glossary' }
          ]
        },
        {
          text: '接口参考',
          items: [{ text: 'API Reference', link: '/zh/api-reference' }]
        }
      ],
      '/en/': [
        {
          text: 'Overview',
          items: [
            { text: 'Home', link: '/en/' },
            { text: 'Quick Start', link: '/en/quick-start' },
            { text: 'Deployment', link: '/en/deploy' },
            { text: 'Configuration', link: '/en/config' }
          ]
        },
        {
          text: 'Usage & Operations',
          items: [
            { text: 'User Guide', link: '/en/user-guide' },
            { text: 'Operations', link: '/en/operations' },
            { text: 'Integrations', link: '/en/integrations' },
            { text: 'Troubleshooting', link: '/en/troubleshooting' },
            { text: 'FAQ', link: '/en/faq' },
            { text: 'Glossary', link: '/en/glossary' }
          ]
        },
        {
          text: 'Reference',
          items: [{ text: 'API Reference', link: '/en/api-reference' }]
        }
      ]
    },
    locales: [
      {
        lang: 'zh',
        outlineTitle: '本页导航',
        lastUpdatedText: '最后更新'
      },
      {
        lang: 'en',
        outlineTitle: 'On this page',
        lastUpdatedText: 'Last updated'
      }
    ]
  }
});
