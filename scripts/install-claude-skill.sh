#!/bin/bash
# Install IronMUD builder skill for Claude Code
#
# This script copies the skill template from templates/claude-skill/ironmud-builder/
# to .claude/skills/ironmud-builder/ where Claude Code will discover it.
#
# Usage: ./scripts/install-claude-skill.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SKILL_SRC="$PROJECT_ROOT/templates/claude-skill/ironmud-builder"
SKILL_DST="$PROJECT_ROOT/.claude/skills/ironmud-builder"

echo "IronMUD Claude Skill Installer"
echo "=============================="
echo ""

# Check source exists
if [ ! -d "$SKILL_SRC" ]; then
    echo "Error: Skill template not found at $SKILL_SRC"
    echo "Make sure you're running this from the IronMUD repository."
    exit 1
fi

# Create destination directory
echo "Creating skill directory..."
mkdir -p "$SKILL_DST"

# Copy files
echo "Copying skill files..."
cp -r "$SKILL_SRC"/* "$SKILL_DST/"

# List installed files
echo ""
echo "Installed files:"
for file in "$SKILL_DST"/*.md; do
    if [ -f "$file" ]; then
        echo "  - $(basename "$file")"
    fi
done

echo ""
echo "Success! IronMUD builder skill installed to:"
echo "  $SKILL_DST"
echo ""
echo "The skill will be active the next time you start Claude Code in this project."
echo ""
echo "Skill includes documentation for:"
echo "  - Core concepts (areas, rooms, items, mobiles, spawn points)"
echo "  - Specialized editors (tedit, pedit, spedit, recedit)"
echo "  - Building patterns and checklists"
echo "  - Game mechanics reference"
