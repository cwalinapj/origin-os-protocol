# Origin OS Protocol Documentation

This directory contains the technical documentation for the Origin OS Protocol.

## Documentation Structure

- **[index.md](index.md)** - Overview and introduction to the protocol
- **[architecture.md](architecture.md)** - Detailed architecture and program specifications
- **[token-economics.md](token-economics.md)** - Token distribution and economics
- **[deployment.md](deployment.md)** - Deployment guide and infrastructure requirements
- **[apps-and-lam.md](apps-and-lam.md)** - Application layer and LAM integration guide

## Current Format: Plain Markdown

This documentation is currently maintained as plain markdown files for simplicity and ease of contribution.

## Future: MkDocs Migration (Optional)

If you want to convert this documentation into a static website using MkDocs:

1. **Install MkDocs**:
   ```bash
   pip install mkdocs mkdocs-material
   ```

2. **Create `mkdocs.yml`** in the project root:
   ```yaml
   site_name: Origin OS Protocol
   site_description: Trustless tokenized rewards system for decentralized AI-powered encrypted hosting + CDN
   site_url: https://cwalinapj.github.io/origin-os-protocol/
   
   theme:
     name: material
     palette:
       primary: indigo
       accent: indigo
   
   nav:
     - Home: index.md
     - Architecture: architecture.md
     - Token Economics: token-economics.md
     - Deployment: deployment.md
     - Apps & LAM: apps-and-lam.md
   
   docs_dir: docs
   ```

3. **Add GitHub Pages workflow** (`.github/workflows/docs.yml`):
   ```yaml
   name: Deploy Documentation
   
   on:
     push:
       branches: [ "main" ]
   
   permissions:
     contents: read
     pages: write
     id-token: write
   
   jobs:
     build:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4
         - uses: actions/setup-python@v4
           with:
             python-version: 3.x
         - run: pip install mkdocs mkdocs-material
         - run: mkdocs build
         - uses: actions/upload-pages-artifact@v2
           with:
             path: site
     
     deploy:
       needs: build
       runs-on: ubuntu-latest
       environment:
         name: github-pages
         url: ${{ steps.deployment.outputs.page_url }}
       steps:
         - id: deployment
           uses: actions/deploy-pages@v2
   ```

4. **Enable GitHub Pages** in repository settings:
   - Go to Settings → Pages
   - Set Source to "GitHub Actions"

**Note**: Adding GitHub Actions workflows requires the credential to have workflow scope. If you encounter permission errors, add the workflow through the GitHub UI instead.

## Contributing

To contribute to documentation:

1. Edit the relevant markdown file
2. Ensure proper formatting and links
3. Submit a pull request with your changes

Keep documentation clear, concise, and up-to-date with code changes.
