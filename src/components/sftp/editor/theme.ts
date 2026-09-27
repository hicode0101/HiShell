/**
 * Fluent 风格 CodeMirror 主题：所有颜色引用 --cm-* CSS 变量，
 * 变量按 html[data-theme] 在 FileEditDialog 中定义，深浅模式自动切换。
 */
import { EditorView } from '@codemirror/view'
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'

export const fluentEditorTheme = EditorView.theme(
  {
    '&': {
      height: '100%',
      fontSize: '13px',
      color: 'var(--cm-fg)',
      backgroundColor: 'transparent',
    },
    '&.cm-focused': {
      outline: 'none',
    },
    '.cm-scroller': {
      fontFamily: "'Cascadia Mono', Consolas, 'Courier New', monospace",
      lineHeight: '1.55',
    },
    '.cm-content': {
      caretColor: 'var(--accent)',
      padding: '8px 0 24px',
    },
    '.cm-gutters': {
      backgroundColor: 'transparent',
      color: 'var(--cm-gutter)',
      border: 'none',
      userSelect: 'none',
    },
    '.cm-activeLine': {
      backgroundColor: 'var(--cm-active)',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'transparent',
      color: 'var(--cm-fg)',
    },
    '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
      backgroundColor: 'var(--cm-selection) !important',
    },
    '.cm-cursor, .cm-dropCursor': {
      borderLeftColor: 'var(--accent)',
      borderLeftWidth: '2px',
    },
    '.cm-matchingBracket': {
      backgroundColor: 'var(--cm-match)',
      outline: '1px solid var(--cm-gutter)',
      color: 'inherit !important',
    },
    '.cm-searchMatch': {
      backgroundColor: 'var(--cm-search)',
      outline: '1px solid var(--cm-searchOutline)',
    },
    '.cm-searchMatch.cm-searchMatch-selected': {
      backgroundColor: 'var(--cm-searchSel)',
    },
    '.cm-panels': {
      backgroundColor: 'var(--bg-layer)',
      color: 'var(--text-primary)',
      borderBottom: '1px solid var(--stroke-divider)',
    },
    '.cm-panel.cm-search': {
      padding: '4px 8px',
      fontSize: '12px',
    },
    '.cm-panel.cm-search input, .cm-panel.cm-search button': {
      fontFamily: 'inherit',
      fontSize: '12px',
      color: 'var(--text-primary)',
      background: 'var(--bg-card)',
      border: '1px solid var(--stroke-control)',
      borderRadius: '4px',
      padding: '2px 6px',
      margin: '1px 2px',
    },
    '.cm-panel.cm-search input:focus, .cm-panel.cm-search button:focus': {
      outline: 'none',
      borderColor: 'var(--accent)',
    },
    '.cm-panel.cm-search label': {
      color: 'var(--text-secondary)',
    },
    '.cm-tooltip': {
      backgroundColor: 'var(--bg-layer)',
      border: '1px solid var(--stroke-card)',
      borderRadius: '6px',
      boxShadow: 'var(--shadow-flyout)',
      overflow: 'hidden',
    },
    '.cm-tooltip-autocomplete ul li[aria-selected]': {
      backgroundColor: 'var(--bg-selected)',
      color: 'var(--text-primary)',
    },
    '.cm-foldGutter span': {
      color: 'var(--cm-gutter)',
      cursor: 'pointer',
      '&:hover': {
        color: 'var(--accent)',
      },
    },
    '.cm-foldPlaceholder': {
      backgroundColor: 'var(--bg-alt-hover)',
      border: '1px solid var(--stroke-control)',
      color: 'var(--text-secondary)',
      borderRadius: '4px',
      padding: '0 6px',
    },
  },
  { dark: false },
)

/** 语法高亮配色（VS Light+/Dark+ 色系，通过 --cm-* 变量适配深浅主题） */
export const fluentHighlight = syntaxHighlighting(
  HighlightStyle.define([
    { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: 'var(--cm-comment)', fontStyle: 'italic' },
    { tag: [t.keyword, t.controlKeyword, t.moduleKeyword, t.operatorKeyword, t.definitionKeyword], color: 'var(--cm-keyword)' },
    { tag: [t.string, t.special(t.string), t.character, t.regexp], color: 'var(--cm-string)' },
    { tag: [t.number, t.integer, t.float, t.bool, t.null, t.atom], color: 'var(--cm-number)' },
    { tag: [t.function(t.variableName), t.function(t.propertyName), t.macroName], color: 'var(--cm-function)' },
    { tag: [t.typeName, t.className, t.namespace, t.self], color: 'var(--cm-type)' },
    { tag: [t.propertyName, t.definition(t.propertyName)], color: 'var(--cm-property)' },
    { tag: [t.variableName, t.definition(t.variableName), t.local(t.variableName), t.standard(t.variableName)], color: 'var(--cm-variable)' },
    { tag: [t.tagName], color: 'var(--cm-tag)' },
    { tag: [t.attributeName], color: 'var(--cm-attr)' },
    { tag: [t.attributeValue], color: 'var(--cm-string)' },
    { tag: [t.operator, t.compareOperator, t.logicOperator, t.arithmeticOperator], color: 'var(--cm-operator)' },
    { tag: [t.punctuation, t.separator, t.bracket, t.paren, t.brace, t.squareBracket], color: 'var(--cm-operator)' },
    { tag: [t.meta, t.processingInstruction, t.annotation], color: 'var(--cm-meta)' },
    { tag: [t.heading], color: 'var(--cm-keyword)', fontWeight: '600' },
    { tag: [t.link, t.url], color: 'var(--cm-string)', textDecoration: 'underline' },
    { tag: [t.emphasis], fontStyle: 'italic' },
    { tag: [t.strong], fontWeight: '700' },
    { tag: [t.invalid], color: 'var(--danger)' },
  ]),
)
