import { EditorState } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';
import { Schema, DOMParser } from 'prosemirror-model';
import { schema as basicSchema } from 'prosemirror-schema-basic';
import { addListNodes } from 'prosemirror-schema-list';
import { exampleSetup } from 'prosemirror-example-setup';
import { keymap } from 'prosemirror-keymap';
import { history } from 'prosemirror-history';
import { baseKeymap } from 'prosemirror-commands';

// Extended schema with comments
const commentNodeSpec = {
  group: 'block',
  attrs: { author: { default: 'agent' }, text: { default: '' } },
  parseDOM: [{ tag: 'div.comment', getAttrs: (dom: Element) => ({
    author: dom.getAttribute('data-author') || 'agent',
    text: dom.textContent || ''
  })}],
  toDOM: (node: any) => ['div', {
    class: `comment ${node.attrs.author}`,
    'data-author': node.attrs.author
  }, ['div', { class: 'author' }, node.attrs.author], ['div', node.attrs.text]]
};

const nodes = addListNodes(basicSchema.spec.nodes, 'paragraph block*', 'block')
  .update('comment', commentNodeSpec);

const marks = basicSchema.spec.marks;

const schema = new Schema({ nodes, marks });

// API connection
const API_BASE = 'http://localhost:3000';

interface PlotState {
  characters: string[];
  locations: string[];
  conflicts: string[];
}

interface Suggestion {
  type: string;
  text: string;
  reasoning?: string;
}

interface Comment {
  author: 'agent' | 'human';
  text: string;
}

// Editor state
let currentChapter = 1;
let totalChapters = 3;
let plotState: PlotState = { characters: [], locations: [], conflicts: [] };
let suggestions: Suggestion[] = [];
let comments: Comment[] = [];

// Initialize editor
const editorElement = document.getElementById('editor')!;
const outlineElement = document.getElementById('outline')!;
const suggestionsElement = document.getElementById('suggestions')!;
const plotStateElement = document.getElementById('plot-state')!;
const commentsElement = document.getElementById('comments')!;
const feedbackInput = document.getElementById('feedback-input') as HTMLInputElement;

const state = EditorState.create({
  schema,
  plugins: exampleSetup({ schema }),
});

const view = new EditorView(editorElement, {
  state,
  dispatchTransaction(tr) {
    view.updateState(view.state.apply(tr));
    updateWordCount();
  }
});

// Update word count
function updateWordCount() {
  const text = view.state.doc.textContent;
  const words = text.split(/\s+/).filter(w => w.length > 0).length;
  document.getElementById('word-count')!.textContent = `Words: ${words}`;
}

// Render outline
function renderOutline() {
  outlineElement.innerHTML = '';
  for (let i = 1; i <= totalChapters; i++) {
    const item = document.createElement('div');
    item.className = `outline-item ${i === currentChapter ? 'active' : ''}`;
    item.innerHTML = `<span class="number">${i}</span><span class="title">Chapter ${i}</span>`;
    item.onclick = () => loadChapter(i);
    outlineElement.appendChild(item);
  }
}

// Load chapter
async function loadChapter(chapterNum: number) {
  currentChapter = chapterNum;
  renderOutline();
  document.getElementById('chapter-status')!.textContent = `Chapter ${chapterNum} of ${totalChapters}`;

  // TODO: Load chapter content from API
  const content = `<h1>Chapter ${chapterNum}</h1><p>Chapter content will be loaded here...</p>`;
  const doc = DOMParser.fromSchema(schema).parse(
    new DOMParser().parseFromString(content, 'text/html').body
  );
  view.updateState(EditorState.create({
    doc,
    plugins: view.state.plugins,
  }));
}

// Save
async function save() {
  const content = view.state.doc.textContent;
  // TODO: Save to API
  console.log('Saving...', content);
}

// Publish
async function publish() {
  // TODO: Publish story
  console.log('Publishing...');
}

// AI Generate
async function aiGenerate() {
  // TODO: Generate with AI
  console.log('Generating...');
}

// AI Continue
async function aiContinue() {
  // TODO: Continue writing
  console.log('Continuing...');
}

// AI Suggest
async function aiSuggest() {
  // TODO: Get suggestions
  console.log('Getting suggestions...');
}

// Toggle bold
function toggleBold() {
  // TODO: Toggle bold mark
}

// Toggle italic
function toggleItalic() {
  // TODO: Toggle italic mark
}

// Toggle heading
function toggleHeading() {
  // TODO: Toggle heading
}

// Toggle quote
function toggleQuote() {
  // TODO: Toggle blockquote
}

// Add comment
function addComment() {
  const text = prompt('Add a comment:');
  if (text) {
    comments.push({ author: 'human', text });
    renderComments();
  }
}

// Send feedback
async function sendFeedback() {
  const feedback = feedbackInput.value;
  if (!feedback) return;

  // TODO: Send feedback to API
  console.log('Feedback:', feedback);
  feedbackInput.value = '';
}

// Apply suggestion
function applySuggestion(index: number) {
  if (index < suggestions.length) {
    const suggestion = suggestions[index];
    // TODO: Apply suggestion to editor
    console.log('Applying:', suggestion);
  }
}

// Render suggestions
function renderSuggestions() {
  suggestionsElement.innerHTML = '';
  suggestions.forEach((suggestion, index) => {
    const item = document.createElement('div');
    item.className = 'ai-suggestion';
    item.onclick = () => applySuggestion(index);
    item.innerHTML = `
      <div class="type">${suggestion.type}</div>
      <div class="text">${suggestion.text}</div>
    `;
    suggestionsElement.appendChild(item);
  });
}

// Render plot state
function renderPlotState() {
  plotStateElement.innerHTML = `
    <p><strong>Characters:</strong> ${plotState.characters.join(', ') || 'None'}</p>
    <p><strong>Location:</strong> ${plotState.locations.join(', ') || 'Unknown'}</p>
    <p><strong>Conflict:</strong> ${plotState.conflicts.join(', ') || 'None'}</p>
  `;
}

// Render comments
function renderComments() {
  commentsElement.innerHTML = '';
  comments.forEach(comment => {
    const item = document.createElement('div');
    item.className = `comment ${comment.author}`;
    item.innerHTML = `
      <div class="author">${comment.author}</div>
      <div>${comment.text}</div>
    `;
    commentsElement.appendChild(item);
  });
}

// Initialize
renderOutline();
renderSuggestions();
renderPlotState();
renderComments();
updateWordCount();
