
import curses
import os
import sys

class Editor:
    def __init__(self, stdscr, filename=None):
        self.stdscr = stdscr
        self.filename = filename
        self.lines = []
        if filename and os.path.exists(filename):
            with open(filename, 'r') as f:
                self.lines = [line.rstrip('\n') for line in f.readlines()]
        if not self.lines:
            self.lines = [""] # Ensure at least one empty line

        self.row = 0
        self.col = 0
        self.mode = 'normal' # 'normal', 'insert', 'command'
        self.command_buffer = ""
        self.should_exit = False

    def run(self):
        self.stdscr.nodelay(1)
        self.stdscr.keypad(True)
        curses.curs_set(1) # Make cursor visible

        while not self.should_exit:
            try:
                max_y, max_x = self.stdscr.getmaxyx()

                # Use noutrefresh and doupdate for smoother updates
                self.stdscr.erase() # Use erase instead of clear
                self.draw_text(max_y, max_x)
                self.draw_status_bar(max_y, max_x)
                self.draw_command_line(max_y, max_x)
                
                self.row = min(self.row, max_y - 2) # -2 for status bar and command line
                self.col = min(self.col, max_x - 1)
                if self.row < len(self.lines):
                    self.col = min(self.col, len(self.lines[self.row]))
                else:
                    self.col = 0

                self.stdscr.move(self.row, self.col)
                curses.doupdate() # Update the physical screen

                key = self.stdscr.getch()
                if key != -1:
                    if self.mode == 'command':
                        self.handle_command_mode_key(key)
                    else:
                        if key == ord('q') and self.mode == 'normal':
                            self.should_exit = True
                        self.handle_key(key)

            except curses.error:
                pass

    def draw_text(self, max_y, max_x):
        for i, line in enumerate(self.lines):
            if i < max_y - 2: # Leave space for status bar and command line
                display_line = line[:max_x]
                try:
                    self.stdscr.addstr(i, 0, display_line.ljust(max_x))
                except curses.error:
                    pass

    def draw_status_bar(self, max_y, max_x):
        mode_str = f"-- {self.mode.upper()} --"
        status_str = f"{self.row+1},{self.col+1}"
        display_status_str = f"{mode_str} {status_str}".ljust(max_x)
        try:
            self.stdscr.addstr(max_y - 2, 0, display_status_str, curses.A_REVERSE)
        except curses.error:
            pass

    def draw_command_line(self, max_y, max_x):
        try:
            self.stdscr.addstr(max_y - 1, 0, f":{self.command_buffer}".ljust(max_x), curses.A_NORMAL)
        except curses.error:
            pass

    def handle_key(self, key):
        if self.mode == 'normal':
            self.handle_normal_mode_key(key)
        elif self.mode == 'insert':
            self.handle_insert_mode_key(key)

    def handle_normal_mode_key(self, key):
        max_y, max_x = self.stdscr.getmaxyx()
        current_line_len = len(self.lines[self.row])

        if key == ord('h') or key == curses.KEY_LEFT:
            self.col = max(0, self.col - 1)
        elif key == ord('l') or key == curses.KEY_RIGHT:
            self.col = min(current_line_len, self.col + 1)
        elif key == ord('j') or key == curses.KEY_DOWN:
            self.row = min(len(self.lines) - 1, self.row + 1)
            self.col = min(len(self.lines[self.row]), self.col)
        elif key == ord('k') or key == curses.KEY_UP:
            self.row = max(0, self.row - 1)
            self.col = min(len(self.lines[self.row]), self.col)
        elif key == ord('i'):
            self.mode = 'insert'
        elif key == ord('x'): # Delete character under cursor
            if self.col < len(self.lines[self.row]):
                current_line = list(self.lines[self.row])
                current_line.pop(self.col)
                self.lines[self.row] = "".join(current_line)
        elif key == ord('d'): # Start of 'dd' command
            self.stdscr.nodelay(0) # Wait for next key
            next_key = self.stdscr.getch()
            self.stdscr.nodelay(1) # Back to non-blocking
            if next_key == ord('d'): # Delete current line
                if len(self.lines) > 1:
                    self.lines.pop(self.row)
                    if self.row >= len(self.lines):
                        self.row = len(self.lines) - 1
                    self.col = min(self.col, len(self.lines[self.row]))
                else:
                    self.lines = [""] # Keep one empty line
                    self.row = 0
                    self.col = 0
        elif key == ord('o'): # Insert new line below
            self.lines.insert(self.row + 1, "")
            self.row += 1
            self.col = 0
            self.mode = 'insert'
        elif key == ord('O'): # Insert new line above
            self.lines.insert(self.row, "")
            self.col = 0
            self.mode = 'insert'
        elif key == ord(':'):
            self.mode = 'command'
            self.command_buffer = ""

    def handle_insert_mode_key(self, key):
        current_line_str = self.lines[self.row]

        if key == curses.KEY_BACKSPACE or key == 127 or key == 8:
            if self.col > 0:
                self.lines[self.row] = current_line_str[:self.col-1] + current_line_str[self.col:]
                self.col -= 1
            elif self.row > 0: # Backspace at beginning of line, join with previous
                prev_line = self.lines.pop(self.row)
                self.row -= 1
                self.col = len(self.lines[self.row])
                self.lines[self.row] += prev_line
        elif key == curses.KEY_ENTER or key == 10:
            # If cursor is at the end of the line, insert a new empty line
            if self.col == len(self.lines[self.row]):
                self.lines.insert(self.row + 1, "")
                self.row += 1
                self.col = 0
            else:
                # Split the current line at the cursor position
                line_before_cursor = self.lines[self.row][:self.col]
                line_after_cursor = self.lines[self.row][self.col:]
                
                # Update the current line to contain only the part before the cursor
                self.lines[self.row] = line_before_cursor
                
                # Insert a new line below the current line with the part after the cursor
                self.lines.insert(self.row + 1, line_after_cursor)
                
                # Move cursor to the beginning of the new line
                self.row += 1
                self.col = 0
        elif key == 27: # ESC key
            self.mode = 'normal'
        elif key == curses.KEY_LEFT: # Handle left arrow key in insert mode
            self.col = max(0, self.col - 1)
        elif key == curses.KEY_RIGHT: # Handle right arrow key in insert mode
            self.col = min(len(self.lines[self.row]), self.col + 1)
        elif key == curses.KEY_UP: # Handle up arrow key in insert mode
            self.row = max(0, self.row - 1)
            self.col = min(len(self.lines[self.row]), self.col)
        elif key == curses.KEY_DOWN: # Handle down arrow key in insert mode
            self.row = min(len(self.lines) - 1, self.row + 1)
            self.col = min(len(self.lines[self.row]), self.col)
        else:
            if 32 <= key <= 126:
                self.lines[self.row] = current_line_str[:self.col] + chr(key) + current_line_str[self.col:]
                self.col += 1

    def handle_command_mode_key(self, key):
        if key == curses.KEY_ENTER or key == 10:
            self.execute_command(self.command_buffer)
            self.command_buffer = ""
            self.mode = 'normal'
        elif key == curses.KEY_BACKSPACE or key == 127 or key == 8:
            self.command_buffer = self.command_buffer[:-1]
        elif key == 27: # ESC key
            self.command_buffer = ""
            self.mode = 'normal'
        else:
            if 32 <= key <= 126:
                self.command_buffer += chr(key)

    def execute_command(self, command):
        if command == 'w':
            self.save_file()
        elif command == 'q':
            self.should_exit = True
        elif command == 'wq':
            self.save_file()
            self.should_exit = True
        elif command.startswith('w '):
            new_filename = command[2:].strip()
            self.save_file(new_filename)
        elif command.startswith('e '):
            new_filename = command[2:].strip()
            self.load_file(new_filename)

    def save_file(self, filename=None):
        target_filename = filename if filename else self.filename
        if not target_filename:
            self.command_buffer = "No filename. Use :w <filename>"
            return
        try:
            with open(target_filename, 'w') as f:
                for line in self.lines:
                    f.write(line + '\n')
            self.command_buffer = f"Saved to {target_filename}"
            self.filename = target_filename
        except Exception as e:
            self.command_buffer = f"Error saving: {e}"

    def load_file(self, filename):
        try:
            with open(filename, 'r') as f:
                self.lines = [line.rstrip('\n') for line in f.readlines()]
            if not self.lines:
                self.lines = [""]
            self.row = 0
            self.col = 0
            self.filename = filename
            self.command_buffer = f"Loaded {filename}"
        except FileNotFoundError:
            self.command_buffer = f"File not found: {filename}"
        except Exception as e:
            self.command_buffer = f"Error loading: {e}"

def main(stdscr):
    filename = None
    if len(sys.argv) > 1:
        filename = sys.argv[1]
    editor = Editor(stdscr, filename)
    editor.run()

# New function for testing without curses wrapper
def run_editor_for_test(input_sequence, initial_lines=None):
    class MockStdscr:
        def __init__(self):
            self._screen = []
            self._cursor_pos = (0, 0)
            self._max_y = 24
            self._max_x = 80

        def addstr(self, y, x, text, attr=0):
            # Simulate drawing on screen
            if y < self._max_y:
                # Ensure the screen has enough rows
                while len(self._screen) <= y:
                    self._screen.append('')
                
                current_line = list(self._screen[y])
                # Ensure the current line has enough columns
                while len(current_line) < x:
                    current_line.extend([' '] * (x - len(current_line)))
                
                # Replace characters at the specified position
                for i, char in enumerate(text):
                    if x + i < self._max_x:
                        if len(current_line) <= x + i:
                            current_line.extend([' '] * (x + i - len(current_line) + 1))
                        current_line[x + i] = char
                self._screen[y] = "".join(current_line)

        def clear(self):
            self._screen = []

        def erase(self):
            self._screen = []

        def getmaxyx(self):
            return self._max_y, self._max_x

        def move(self, y, x):
            self._cursor_pos = (y, x)

        def refresh(self):
            pass # No actual refresh in mock

        def nodelay(self, arg):
            pass

        def keypad(self, arg):
            pass

        def getch(self):
            return -1 # Not used in simulated input

    mock_stdscr = MockStdscr()
    editor = Editor(mock_stdscr)
    if initial_lines:
        editor.lines = initial_lines

    for key_code in input_sequence:
        editor.handle_key(key_code)

    return editor.lines

if __name__ == '__main__':
    try:
        # Check if running in a test context
        if len(sys.argv) > 1 and sys.argv[1] == "--test":
            # This block is for direct testing calls, not for curses.wrapper
            # The test scripts will call run_editor_for_test directly
            pass
        else:
            curses.wrapper(main)
    except curses.error as e:
        print(f"Error initializing curses: {e}. Please ensure your terminal supports curses and is large enough.")


