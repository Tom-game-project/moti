import curses
import os
import sys

class Editor:
    def __init__(self, stdscr, initial_filename=None):
        self.stdscr = stdscr
        self.should_exit = False
        self.mode = 'normal'  # 'normal', 'insert', 'command'
        self.command_buffer = ""

        # --- Buffer Management ---
        self.buffers = []
        self.active_buffer_index = -1
        if initial_filename:
            self._open_file_in_new_buffer(initial_filename)
        else:
            self._open_file_in_new_buffer(None) # Start with an empty buffer

        # --- Directory Tree Properties ---
        self.tree_visible = True
        self.tree_view_active = True
        self.tree_width = 30
        self.current_path = os.getcwd()
        self.tree_scroll_pos = 0
        self.selected_item_index = 0
        self.expanded_dirs = {self.current_path}
        self.tree_items = []

    @property
    def active_buffer(self):
        if 0 <= self.active_buffer_index < len(self.buffers):
            return self.buffers[self.active_buffer_index]
        return None

    def run(self):
        self.stdscr.nodelay(1)
        self.stdscr.keypad(True)
        curses.curs_set(1)

        while not self.should_exit:
            try:
                max_y, max_x = self.stdscr.getmaxyx()
                self.stdscr.erase()

                tree_pane_width = self.tree_width if self.tree_visible else 0
                
                if self.tree_visible:
                    self._update_tree_items()
                    self.draw_tree_view(max_y, max_x)
                    separator_x = self.tree_width
                    for y in range(max_y - 2):
                        try:
                            self.stdscr.addch(y, separator_x, curses.ACS_VLINE)
                        except curses.error:
                            pass
                
                editor_win_x = tree_pane_width + 1 if self.tree_visible else 0
                editor_win_width = max_x - editor_win_x
                editor_win_height = max_y - 2
                
                if editor_win_width > 0 and editor_win_height > 0:
                    editor_win = self.stdscr.subwin(editor_win_height, editor_win_width, 0, editor_win_x)
                    if self.active_buffer:
                        self.scroll_text(editor_win_height)
                        self.draw_text(editor_win, editor_win_height, editor_win_width)

                self.draw_status_bar(max_y, max_x)
                self.draw_command_line(max_y, max_x)
                
                if self.tree_view_active and self.tree_visible:
                    curses.curs_set(0)
                elif self.active_buffer:
                    curses.curs_set(1)
                    buf = self.active_buffer
                    line_num_width = len(str(len(buf['lines']))) + 2
                    
                    # Constrain cursor to valid positions within the buffer
                    buf['row'] = max(0, min(buf['row'], len(buf['lines']) - 1))
                    current_line_len = len(buf['lines'][buf['row']])
                    buf['col'] = max(0, min(buf['col'], current_line_len))
                    
                    # Adjust column based on visible editor width
                    buf['col'] = max(0, min(buf['col'], editor_win_width - line_num_width - 1))

                    # The cursor's on-screen Y position is relative to the scroll position
                    screen_y = buf['row'] - buf['top_row']
                    self.stdscr.move(screen_y, editor_win_x + buf['col'] + line_num_width)
                else:
                    curses.curs_set(0)

                curses.doupdate()

                key = self.stdscr.getch()
                if key != -1:
                    if self.mode == 'command':
                        self.handle_command_mode_key(key)
                    elif self.tree_view_active and self.tree_visible:
                        self.handle_tree_view_key(key)
                    else:
                        self.handle_key(key)

            except curses.error:
                pass
            except Exception:
                self.should_exit = True

    def scroll_text(self, editor_win_height):
        buf = self.active_buffer
        if not buf: return

        # Scroll up if cursor is above the visible area
        if buf['row'] < buf['top_row']:
            buf['top_row'] = buf['row']
        # Scroll down if cursor is below the visible area
        if buf['row'] >= buf['top_row'] + editor_win_height:
            buf['top_row'] = buf['row'] - editor_win_height + 1

    def _get_tree_items(self, path, prefix=""):
        items = []
        try:
            entries = sorted(os.listdir(path))
            dirs = sorted([d for d in entries if os.path.isdir(os.path.join(path, d))])
            files = sorted([f for f in entries if os.path.isfile(os.path.join(path, f))])
            
            for item_name in dirs + files:
                full_path = os.path.join(path, item_name)
                is_dir = os.path.isdir(full_path)
                items.append({'path': full_path, 'prefix': prefix, 'is_dir': is_dir})
                if is_dir and full_path in self.expanded_dirs:
                    items.extend(self._get_tree_items(full_path, prefix + "  "))
        except PermissionError:
            items.append({'path': os.path.join(path, "[Permission Denied]"), 'prefix': prefix, 'is_dir': False})
        return items

    def _update_tree_items(self):
        self.tree_items = self._get_tree_items(self.current_path)

    def draw_tree_view(self, max_y, max_x):
        tree_win = self.stdscr.subwin(max_y - 2, self.tree_width, 0, 0)
        tree_win.leaveok(True)
        tree_win.erase()

        for i, item in enumerate(self.tree_items[self.tree_scroll_pos:]):
            if i >= max_y - 2: break
            
            indicator = "-" if item['path'] in self.expanded_dirs else "+"
            display_text = f"{item['prefix']}{indicator} {os.path.basename(item['path'])}" if item['is_dir'] else f"{item['prefix']}  {os.path.basename(item['path'])}"
            display_text = display_text[:self.tree_width-1]

            attr = curses.A_REVERSE if i + self.tree_scroll_pos == self.selected_item_index else curses.A_NORMAL
            tree_win.addstr(i, 0, display_text.ljust(self.tree_width), attr)
        
        tree_win.noutrefresh()

    def draw_text(self, editor_win, max_y, max_x):
        buf = self.active_buffer
        line_num_width = len(str(len(buf['lines']))) + 2
        editor_win.leaveok(True)
        editor_win.erase()
        for i in range(max_y):
            file_line_index = buf['top_row'] + i
            if file_line_index < len(buf['lines']):
                line = buf['lines'][file_line_index]
                line_num_str = f"{file_line_index + 1}".rjust(line_num_width - 1) + " "
                display_line = line[:max_x - line_num_width]
                try:
                    editor_win.addstr(i, 0, line_num_str, curses.A_REVERSE)
                    editor_win.addstr(i, line_num_width, display_line.ljust(max_x - line_num_width))
                except curses.error:
                    pass
        editor_win.noutrefresh()

    def draw_status_bar(self, max_y, max_x):
        buf = self.active_buffer
        if not buf: return

        mode_str = f"-- {self.mode.upper()} --"
        filename = os.path.basename(buf['filename']) if buf['filename'] else "[No Name]"
        modified_str = " [+]" if buf['modified'] else ""
        
        left_status = f"{mode_str} {filename}{modified_str}"
        right_status = f"{buf['row']+1},{buf['col']+1}"
        
        buffer_info = f"[{self.active_buffer_index + 1}/{len(self.buffers)}]"
        
        status_text = f"{left_status} {buffer_info}".ljust(max_x - len(right_status) -1) + f" {right_status}"
        status_text = status_text[:max_x]

        try:
            self.stdscr.addstr(max_y - 2, 0, status_text, curses.A_REVERSE)
        except curses.error:
            pass

    def draw_command_line(self, max_y, max_x):
        try:
            self.stdscr.addstr(max_y - 1, 0, f":{self.command_buffer}".ljust(max_x), curses.A_NORMAL)
        except curses.error:
            pass

    def handle_tree_view_key(self, key):
        if key in (ord('j'), curses.KEY_DOWN):
            self.selected_item_index = min(len(self.tree_items) - 1, self.selected_item_index + 1)
        elif key in (ord('k'), curses.KEY_UP):
            self.selected_item_index = max(0, self.selected_item_index - 1)
        elif key in (curses.KEY_ENTER, 10):
            if self.selected_item_index < len(self.tree_items):
                selected = self.tree_items[self.selected_item_index]
                if selected['is_dir']:
                    if selected['path'] in self.expanded_dirs:
                        self.expanded_dirs.remove(selected['path'])
                    else:
                        self.expanded_dirs.add(selected['path'])
                    self._update_tree_items()
                else:
                    self.open_file(selected['path'])
                    self.tree_view_active = False
        elif key == ord('q'):
            self.should_exit = True
        elif key == 9: # Tab
            self.tree_view_active = False

    def handle_key(self, key):
        if not self.active_buffer:
            if key == ord(':'):
                self.mode = 'command'
                self.command_buffer = ""
            elif key == 9: # Tab
                if self.tree_visible:
                    self.tree_view_active = True
            return

        if self.mode == 'normal':
            self.handle_normal_mode_key(key)
        elif self.mode == 'insert':
            self.handle_insert_mode_key(key)

    def handle_normal_mode_key(self, key):
        buf = self.active_buffer

        if key in (ord('h'), curses.KEY_LEFT):
            buf['col'] = max(0, buf['col'] - 1)
        elif key in (ord('l'), curses.KEY_RIGHT):
            current_line_len = len(buf['lines'][buf['row']])
            buf['col'] = min(current_line_len, buf['col'] + 1)
        elif key in (ord('j'), curses.KEY_DOWN):
            buf['row'] = min(len(buf['lines']) - 1, buf['row'] + 1)
            buf['col'] = min(len(buf['lines'][buf['row']]), buf['col'])
        elif key in (ord('k'), curses.KEY_UP):
            buf['row'] = max(0, buf['row'] - 1)
            buf['col'] = min(len(buf['lines'][buf['row']]), buf['col'])
        elif key == ord('i'):
            self.mode = 'insert'
        elif key == ord('x'):
            if buf['col'] < len(buf['lines'][buf['row']]):
                current_line = list(buf['lines'][buf['row']])
                current_line.pop(buf['col'])
                buf['lines'][buf['row']] = "".join(current_line)
                buf['modified'] = True
        elif key == ord('d'):
            self.stdscr.nodelay(0)
            next_key = self.stdscr.getch()
            self.stdscr.nodelay(1)
            if next_key == ord('d'):
                if len(buf['lines']) > 1:
                    buf['lines'].pop(buf['row'])
                    if buf['row'] >= len(buf['lines']):
                        buf['row'] = len(buf['lines']) - 1
                else:
                    buf['lines'] = [""]
                    buf['row'] = 0
                buf['col'] = min(buf['col'], len(buf['lines'][buf['row']]))
                buf['modified'] = True
        elif key == ord('o'):
            buf['lines'].insert(buf['row'] + 1, "")
            buf['row'] += 1
            buf['col'] = 0
            self.mode = 'insert'
            buf['modified'] = True
        elif key == ord('O'):
            buf['lines'].insert(buf['row'], "")
            buf['col'] = 0
            self.mode = 'insert'
            buf['modified'] = True
        elif key == ord(':'):
            self.mode = 'command'
            self.command_buffer = ""
        elif key == 9: # Tab
            if self.tree_visible:
                self.tree_view_active = True

    def handle_insert_mode_key(self, key):
        buf = self.active_buffer
        current_line_str = buf['lines'][buf['row']]
        buf['modified'] = True

        if key in (curses.KEY_BACKSPACE, 127, 8):
            if buf['col'] > 0:
                buf['lines'][buf['row']] = current_line_str[:buf['col']-1] + current_line_str[buf['col']:]
                buf['col'] -= 1
            elif buf['row'] > 0:
                prev_line = buf['lines'].pop(buf['row'])
                buf['row'] -= 1
                buf['col'] = len(buf['lines'][buf['row']])
                buf['lines'][buf['row']] += prev_line
        elif key in (curses.KEY_ENTER, 10):
            line_after_cursor = current_line_str[buf['col']:]
            buf['lines'][buf['row']] = current_line_str[:buf['col']]
            buf['lines'].insert(buf['row'] + 1, line_after_cursor)
            buf['row'] += 1
            buf['col'] = 0
        elif key == 27: # ESC
            self.mode = 'normal'
        elif key == curses.KEY_LEFT:
            buf['col'] = max(0, buf['col'] - 1)
        elif key == curses.KEY_RIGHT:
            buf['col'] = min(len(buf['lines'][buf['row']]), buf['col'] + 1)
        elif key == curses.KEY_UP:
            buf['row'] = max(0, buf['row'] - 1)
            buf['col'] = min(len(buf['lines'][buf['row']]), buf['col'])
        elif key == curses.KEY_DOWN:
            buf['row'] = min(len(buf['lines']) - 1, buf['row'] + 1)
            buf['col'] = min(len(buf['lines'][buf['row']]), buf['col'])
        elif 32 <= key <= 126:
            buf['lines'][buf['row']] = current_line_str[:buf['col']] + chr(key) + current_line_str[buf['col']:]
            buf['col'] += 1

    def handle_command_mode_key(self, key):
        if key in (curses.KEY_ENTER, 10):
            self.execute_command(self.command_buffer)
            self.command_buffer = ""
            self.mode = 'normal'
        elif key in (curses.KEY_BACKSPACE, 127, 8):
            self.command_buffer = self.command_buffer[:-1]
        elif key == 27: # ESC
            self.command_buffer = ""
            self.mode = 'normal'
        elif 32 <= key <= 126:
            self.command_buffer += chr(key)

    def execute_command(self, command):
        parts = command.split()
        if not parts: return
        cmd = parts[0]
        args = parts[1:]

        if cmd == 'w':
            self.save_file(args[0] if args else None)
        elif cmd == 'q':
            unsaved = [os.path.basename(b['filename']) for b in self.buffers if b['modified'] and b['filename']]
            if unsaved:
                self.command_buffer = f"Unsaved changes in: {', '.join(unsaved)}. Use q! to force."
                return
            self.should_exit = True
        elif cmd == 'q!':
            self.should_exit = True
        elif cmd == 'wq':
            self.save_file()
            if not self.active_buffer or not self.active_buffer['modified']:
                self.should_exit = True
        elif cmd == 'e':
            if args: self.open_file(args[0])
        elif cmd == 'bn':
            if len(self.buffers) > 1:
                self.active_buffer_index = (self.active_buffer_index + 1) % len(self.buffers)
        elif cmd == 'bp':
            if len(self.buffers) > 1:
                self.active_buffer_index = (self.active_buffer_index - 1 + len(self.buffers)) % len(self.buffers)
        elif cmd == 'tt':
            self.tree_visible = not self.tree_visible
            if not self.tree_visible:
                self.tree_view_active = False

    def save_file(self, filename=None):
        buf = self.active_buffer
        if not buf: return

        target_filename = filename if filename else buf['filename']
        if not target_filename:
            self.command_buffer = "No filename. Use :w <filename>"
            return
        try:
            with open(target_filename, 'w') as f:
                f.write('\n'.join(buf['lines']))
            buf['filename'] = target_filename
            buf['modified'] = False
            self.command_buffer = f"Saved to {target_filename}"
        except Exception as e:
            self.command_buffer = f"Error saving: {e}"

    def open_file(self, filename):
        abs_path = os.path.abspath(filename)
        for i, buf in enumerate(self.buffers):
            if buf.get('filename') == abs_path:
                self.active_buffer_index = i
                return
        
        self._open_file_in_new_buffer(abs_path)

    def _open_file_in_new_buffer(self, filename):
        new_buffer = {
            'filename': filename,
            'lines': [""],
            'row': 0,
            'col': 0,
            'top_row': 0, # Initialize scroll position
            'modified': False
        }
        if filename and os.path.exists(filename):
            try:
                with open(filename, 'r') as f:
                    new_buffer['lines'] = [line.rstrip('\n') for line in f.readlines()]
                if not new_buffer['lines']:
                    new_buffer['lines'] = [""]
            except Exception as e:
                self.command_buffer = f"Error loading {filename}: {e}"
        
        self.buffers.append(new_buffer)
        self.active_buffer_index = len(self.buffers) - 1
        self.command_buffer = f"Opened {filename}" if filename else "Opened new buffer"

def main(stdscr):
    initial_file = sys.argv[1] if len(sys.argv) > 1 else None
    editor = Editor(stdscr, initial_file)
    editor.run()

if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == "--test":
        print("Test mode is not implemented in this version.")
    else:
        try:
            curses.wrapper(main)
        except curses.error as e:
            print(f"Error: {e}")
            print("Failed to initialize curses. Your terminal might not be supported.")
        except Exception as e:
            print(f"An unexpected error occurred: {e}")
