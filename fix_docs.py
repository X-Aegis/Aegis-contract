import re

with open("smartcontract/contracts/volatility_shield/src/lib.rs", "r") as f:
    lines = f.readlines()

new_lines = []
for i, line in enumerate(lines):
    # If line is a pub fn inside VolatilityShield, check if previous line is a docstring or a macro
    if re.match(r'^\s*pub fn \w+\(', line):
        prev_idx = i - 1
        has_doc = False
        while prev_idx >= 0:
            prev_line = lines[prev_idx].strip()
            if prev_line.startswith("///"):
                has_doc = True
                break
            if not prev_line.startswith("#") and prev_line != "":
                break
            prev_idx -= 1
        
        if not has_doc:
            func_name = re.match(r'^\s*pub fn (\w+)\(', line).group(1)
            indent = line[:len(line) - len(line.lstrip())]
            new_lines.append(f"{indent}/// {func_name.replace('_', ' ').capitalize()} function.\n")
    
    # Check for enum / struct without docs
    if re.match(r'^\s*pub enum \w+', line) or re.match(r'^\s*pub struct \w+', line):
        prev_idx = i - 1
        has_doc = False
        while prev_idx >= 0:
            prev_line = lines[prev_idx].strip()
            if prev_line.startswith("///"):
                has_doc = True
                break
            if not prev_line.startswith("#") and prev_line != "":
                break
            prev_idx -= 1
        
        if not has_doc:
            item_name = re.match(r'^\s*pub (?:enum|struct) (\w+)', line).group(1)
            indent = line[:len(line) - len(line.lstrip())]
            new_lines.append(f"{indent}/// {item_name} structure.\n")

    new_lines.append(line)

with open("smartcontract/contracts/volatility_shield/src/lib.rs", "w") as f:
    f.writelines(new_lines)
