import { Fragment, useMemo, useState } from "react";
import type { AvailableSkillOption } from "../types";
import { compactSkillPath, toSkillSourceRoot } from "../lib/format";

/* 把一个 skill 的来源目录(去重)解析出来；paths[0] 对应 claude_skill_sources 里第一个命中的源，
 * 也就是引擎 find_skill_source 实际会链接的那一份。 */
function sourceRoots(option: AvailableSkillOption | undefined): string[] {
  return Array.from(new Set((option?.paths ?? []).map(toSkillSourceRoot)));
}

function skillDisplayName(option: AvailableSkillOption | undefined, fallback: string): string {
  return option?.display_name?.trim() || fallback;
}

function skillDescription(option: AvailableSkillOption | undefined): string | undefined {
  return option?.description?.trim() || undefined;
}

export function SkillCheckboxList({
  availableSkills,
  selectedSkills,
  skillSourcesLabel,
  selectedSummaryLabel,
  emptySelectionLabel,
  unselectLabel,
  onChange,
}: {
  availableSkills: AvailableSkillOption[];
  selectedSkills: string[];
  skillSourcesLabel: string;
  selectedSummaryLabel: string;
  emptySelectionLabel: string;
  unselectLabel: (skill: string) => string;
  onChange: (next: string[]) => void;
}) {
  const optionMap = useMemo(
    () => new Map(availableSkills.map((option) => [option.name, option])),
    [availableSkills]
  );
  const allSkills = useMemo(
    () => Array.from(new Set([...availableSkills.map((option) => option.name), ...selectedSkills]))
      .sort((a, b) => skillDisplayName(optionMap.get(a), a).localeCompare(skillDisplayName(optionMap.get(b), b))),
    [availableSkills, optionMap, selectedSkills]
  );

  /* 按主来源(primary root)分组；无来源的(配置了但磁盘找不到)归到最后一组。 */
  const groups = useMemo(() => {
    const map = new Map<string, string[]>();
    for (const skill of allSkills) {
      const primary = sourceRoots(optionMap.get(skill))[0] ?? "";
      const list = map.get(primary) ?? [];
      list.push(skill);
      map.set(primary, list);
    }
    return Array.from(map.entries())
      .map(([root, skills]) => ({ root, skills }))
      .sort((a, b) => {
        if (a.root === "") return 1;
        if (b.root === "") return -1;
        return a.root.localeCompare(b.root);
      });
  }, [allSkills, optionMap]);

  const [collapsedRoots, setCollapsedRoots] = useState<Set<string>>(() => new Set());
  const toggleGroup = (root: string) =>
    setCollapsedRoots((prev) => {
      const next = new Set(prev);
      if (next.has(root)) {
        next.delete(root);
      } else {
        next.add(root);
      }
      return next;
    });

  return (
    <div className="checkbox-list">
      <div className="checkbox-selected-bar">
        <div className="checkbox-selected-head">
          <span>{selectedSummaryLabel}</span>
          <strong>{selectedSkills.length}</strong>
        </div>
        <div className="checkbox-selected-list">
          {selectedSkills.length ? (
            selectedSkills.map((skill) => {
              const option = optionMap.get(skill);
              const displayName = skillDisplayName(option, skill);
              const description = skillDescription(option);
              return (
                <span key={`selected-${skill}`} className="skill-chip" title={description ?? skill}>
                  <span className="skill-chip-label">{displayName}</span>
                  {displayName !== skill && <code className="skill-chip-id">{skill}</code>}
                  <button
                    type="button"
                    className="skill-chip-remove"
                    onClick={() => onChange(selectedSkills.filter((s) => s !== skill))}
                    aria-label={unselectLabel(skill)}
                  >
                    ×
                  </button>
                </span>
              );
            })
          ) : (
            <span className="muted">{emptySelectionLabel}</span>
          )}
        </div>
      </div>

      {groups.map((group) => {
        const isCollapsed = collapsedRoots.has(group.root);
        return (
        <Fragment key={group.root || "__none__"}>
          <button
            type="button"
            className="checkbox-group-head"
            aria-expanded={!isCollapsed}
            onClick={() => toggleGroup(group.root)}
          >
            <span className="checkbox-group-caret" aria-hidden="true">›</span>
            {group.root ? (
              <code className="checkbox-group-source" title={group.root}>
                {compactSkillPath(group.root)}
              </code>
            ) : (
              <span className="muted checkbox-group-source">{skillSourcesLabel}</span>
            )}
            <span className="checkbox-group-count">{group.skills.length}</span>
          </button>
          {!isCollapsed && group.skills.map((skill) => {
            const option = optionMap.get(skill);
            const isSelected = selectedSkills.includes(skill);
            const roots = sourceRoots(option);
            const extraSources = roots.length - 1;
            const displayName = skillDisplayName(option, skill);
            const description = skillDescription(option);
            return (
              <label key={skill} className="checkbox-item" title={description ?? skill}>
                <div className="checkbox-line">
                  <input
                    type="checkbox"
                    checked={isSelected}
                    onChange={(event) => {
                      if (event.target.checked) {
                        onChange([...selectedSkills, skill].sort());
                      } else {
                        onChange(selectedSkills.filter((s) => s !== skill));
                      }
                    }}
                  />
                  <span className="checkbox-title">
                    <span className="checkbox-name">{displayName}</span>
                    {displayName !== skill && <code className="checkbox-id">{skill}</code>}
                  </span>
                  {extraSources > 0 && (
                    <span
                      className="checkbox-extra"
                      title={roots.join("\n")}
                    >
                      +{extraSources}
                    </span>
                  )}
                </div>
                {description && <p className="checkbox-description">{description}</p>}
              </label>
            );
          })}
        </Fragment>
        );
      })}
    </div>
  );
}
