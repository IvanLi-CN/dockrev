---
title: 公开 Demo
description: 公开 Demo 入口与说明。
---

# 公开 Demo

- 中文说明：[公开 Demo（中文）](/zh/demo)
- English guide: [Public Demo (English)](/en/demo)
- 入口：[https://dockrev-demo.ivanli.cc/](https://dockrev-demo.ivanli.cc/)

## 发布与配置

- 只有 `Release` 会构建并发布 demo 站点；PR 只做 build + smoke。
- 首次启用需要 owner 在 EdgeOne Makers 建好 direct-upload 项目与域名绑定，并配置仓库变量 `DOCKREV_DEMO_PUBLISH_ENABLED=true`、secret `EDGEONE_API_TOKEN` 与 `DOCKREV_DEMO_EDGEONE_PROJECT`。未启用时 Release 仍产出 `dockrev-demo-web-<version>` artifact，可手工部署。
