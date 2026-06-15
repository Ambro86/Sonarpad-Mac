import re

path = r"C:\rustnotepad\sonarpad_mobile_starter\tv_channels_resolver.php"
with open(path, "r", encoding="utf-8") as f:
    content = f.read()

# 1. Rename NOVE to La9 in the JSON block
content = content.replace('"name": "[9] NOVE",', '"name": "[9] La9",')
content = content.replace('"tvg_name": "NOVE",', '"tvg_name": "La9",')

# 2. Remove the foreach loop that resolves aurora_channels
loop_pattern = r"foreach \(\$channels as &\$channel\) \{.*?\bunset\(\$channel\);"
content = re.sub(loop_pattern, "", content, flags=re.DOTALL)

with open(path, "w", encoding="utf-8") as f:
    f.write(content)

print("PHP modificato con successo.")
