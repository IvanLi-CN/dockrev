import { defineConfig } from 'rspress/config';

function normalizeBase(base: string | undefined): string {
  const raw = (base ?? '/').trim();
  if (!raw || raw === '/') return '/';
  const withLeading = raw.startsWith('/') ? raw : `/${raw}`;
  return withLeading.endsWith('/') ? withLeading : `${withLeading}/`;
}

const docsBase = normalizeBase(process.env.DOCS_BASE);
const withDocsBase = (assetPath: string): string => {
  const normalizedAssetPath = assetPath.startsWith('/') ? assetPath.slice(1) : assetPath;
  return `${docsBase}${normalizedAssetPath}`;
};

export default defineConfig({
  root: 'docs',
  base: docsBase,
  title: 'Dockrev Documentation',
  description: 'Dockrev deployment, operations, and API reference documentation.',
  icon: '/favicon.svg',
  logo: '/dockrev-logo.svg',
  logoText: '',
  lang: 'zh',
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: withDocsBase('favicon.svg') }],
    ['link', { rel: 'icon', type: 'image/png', href: withDocsBase('favicon.png') }],
    ['link', { rel: 'icon', href: withDocsBase('favicon.ico'), sizes: 'any' }],
    ['link', { rel: 'apple-touch-icon', href: withDocsBase('apple-touch-icon.png') }],
    ['meta', { property: 'og:title', content: 'Dockrev' }],
    ['meta', { property: 'og:description', content: 'Self-hosted Docker/Compose update manager' }],
    ['meta', { property: 'og:image', content: withDocsBase('dockrev-social-preview.png') }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['meta', { name: 'twitter:title', content: 'Dockrev' }],
    ['meta', { name: 'twitter:description', content: 'Self-hosted Docker/Compose update manager' }],
    ['meta', { name: 'twitter:image', content: withDocsBase('dockrev-social-preview.png') }]
  ],
  themeConfig: {
    search: true,
    nav: [
      {
        text: '中文',
        link: '/index.html',
        activeMatch: '^/(?!en(?:/|$)).*'
      },
      {
        text: 'English',
        link: '/en/index.html',
        activeMatch: '^/en(?:/|$)'
      },
      { text: 'Storybook', link: '/storybook.html' },
      { text: 'GitHub', link: 'https://github.com/ivanli-cn/dockrev' }
    ],
    sidebar: {
      '/': [
        {
          text: '总览',
          items: [
            { text: '首页', link: '/' },
            { text: '快速开始', link: '/quick-start' },
            { text: '部署指南', link: '/deploy' },
            { text: '配置参考', link: '/config' }
          ]
        },
        {
          text: '使用与运维',
          items: [
            { text: '用户使用手册', link: '/user-guide' },
            { text: '运维手册', link: '/operations' },
            { text: '集成指南', link: '/integrations' },
            { text: '故障排查', link: '/troubleshooting' },
            { text: '常见问题', link: '/faq' },
            { text: '术语表', link: '/glossary' }
          ]
        },
        {
          text: '接口参考',
          items: [
            { text: 'API Reference', link: '/api-reference' },
            { text: 'Notifications', link: '/notifications' }
          ]
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
          items: [
            { text: 'API Reference', link: '/en/api-reference' },
            { text: 'Notifications', link: '/en/notifications' }
          ]
        }
      ]
    }
  }
});
