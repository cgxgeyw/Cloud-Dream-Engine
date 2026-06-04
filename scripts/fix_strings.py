import sys
sys.stdout.reconfigure(encoding='utf-8')

# Write strings.xml without BOM
content = '<resources>\n    <string name="app_name">云朵梦境</string>\n    <string name="main_activity_title">云朵梦境</string>\n</resources>\n'

with open(r'E:\code\rustweb\src-tauri\gen\android\app\src\main\res\values\strings.xml', 'w', encoding='utf-8') as f:
    f.write(content)

# Verify
with open(r'E:\code\rustweb\src-tauri\gen\android\app\src\main\res\values\strings.xml', 'rb') as f:
    raw = f.read()
    print('Has BOM:', raw[:3] == b'\xef\xbb\xbf')
    print('Content:', raw.decode('utf-8'))
