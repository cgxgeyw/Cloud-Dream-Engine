import re, sys
t=open(r'C:\Users\HS\AppData\Local\Temp\ui6.xml',encoding='utf-8',errors='replace').read()
for m in re.finditer(r'<node[^>]+>', t):
    s=m.group(0)
    if 'EditText' in s or '发送' in s or '添加图片' in s or '语音输入' in s:
        print(s[:600])
        print('---')
