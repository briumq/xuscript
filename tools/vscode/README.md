# Xu Language VS Code Extension

This extension provides syntax highlighting and language configuration for the **Xu Script** language (`.xu`).

## Features

- **Syntax Highlighting**: Comprehensive coloring for keywords, strings, comments, operators, and types based on Xu Language Spec v1.1.
- **Bracket Matching**: Automatic matching and closing for braces, brackets, and parentheses.
- **Commenting**: Support for single-line (`//`) and block (`/* */`) comments.
- **String Interpolation**: Highlighting for embedded expressions inside strings (`{expr}`).

## Installation

1. Clone the repository.
2. Open the `tools/vscode` folder in VS Code.
3. Press `F5` to launch a new Extension Development Host window with the plugin loaded.
4. Open any `.xu` file to see syntax highlighting in action.

## Packaging

To create a `.vsix` file for installation:

1. Install `vsce`: `npm install -g vsce`
2. Run `vsce package` in this directory.
