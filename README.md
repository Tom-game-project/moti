# Python Vim-like Editor

This is a lightweight, Vim-like text editor implemented in Python using the `curses` library. It features a built-in file explorer tree, allowing you to easily navigate and edit files within your project.

## How to Run

To start the editor, simply run the `editor.py` script from your terminal:

```bash
python editor.py
```

You can also open a specific file by passing its name as an argument:

```bash
python editor.py /path/to/your/file.txt
```

## Key Bindings

The editor has two main modes of operation: **Normal Mode** and **Insert Mode**. It also features a **Tree View** for file navigation.

### 🌳 Tree View

The editor starts in the Tree View, which is displayed on the left side of the screen.

| Key | Action |
| :--- | :--- |
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Enter` | - **On a directory**: Expand or collapse the directory.<br>- **On a file**: Open the file in the editor view. |
| `Tab` | Switch focus between the Tree View and the Editor View. |
| `q` | Quit the application. |

### モード共通

| Key | Action |
| :--- | :--- |
| `Tab` | Switch focus between the Tree View and the Editor View. |
| `:q` | Quit the application. |
| `:w` | Save the current file. |
| `:wq` | Save and quit. |

###  Normal Mode (Editor View)

This is the default mode for navigating and manipulating text.

| Key | Action |
| :--- | :--- |
| `h` / `←` | Move cursor left |
| `j` / `↓` | Move cursor down |
| `k` / `↑` | Move cursor up |
| `l` / `→` | Move cursor right |
| `i` | Enter **Insert Mode** at the current cursor position. |
| `o` | Insert a new line below the current line and enter Insert Mode. |
| `O` | Insert a new line above the current line and enter Insert Mode. |
| `x` | Delete the character under the cursor. |
| `dd` | Delete the current line. |
| `:` | Enter **Command Mode** (e.g., for `:w`, `:q`). |

### ✏️ Insert Mode (Editor View)

This mode is for typing and editing text.

| Key | Action |
| :--- | :--- |
| `Esc` | Return to **Normal Mode**. |
| `Backspace` | Delete the character before the cursor. |
| `Enter` | Insert a new line. |
| (Other keys) | Insert characters at the cursor position. |

## How to Quit

- In **Normal Mode**, type `:q` and press `Enter`.
- If you have unsaved changes, you can save and quit with `:wq`.
- From the **Tree View**, simply press `q`.