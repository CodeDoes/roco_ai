"use client";

import { useEffect, useRef } from "react";
import { EditorState } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { Schema, DOMParser } from "prosemirror-model";
import { schema as basicSchema } from "prosemirror-schema-basic";
import { exampleSetup } from "prosemirror-example-setup";

interface ProseMirrorEditorProps {
  content: string;
  onChange: (content: string) => void;
}

export function ProseMirrorEditor({ content, onChange }: ProseMirrorEditorProps) {
  const editorRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);

  useEffect(() => {
    if (!editorRef.current) return;

    const schema = basicSchema;
    const plugins = exampleSetup({ schema });

    const state = EditorState.create({
      doc: DOMParser.fromSchema(schema).parse(
        new DOMParser().parseFromString(content || "<p></p>", "text/html").body
      ),
      plugins,
    });

    const view = new EditorView(editorRef.current, {
      state,
      dispatchTransaction(tr) {
        const newState = view.state.apply(tr);
        view.updateState(newState);
        onChange(newState.doc.textContent);
      },
    });

    viewRef.current = view;

    return () => {
      view.destroy();
    };
  }, []);

  useEffect(() => {
    if (viewRef.current && content !== viewRef.current.state.doc.textContent) {
      const schema = basicSchema;
      const doc = DOMParser.fromSchema(schema).parse(
        new DOMParser().parseFromString(content || "<p></p>", "text/html").body
      );
      const tr = viewRef.current.state.tr.replaceWith(
        0,
        viewRef.current.state.doc.content.size,
        doc.content
      );
      viewRef.current.dispatch(tr);
    }
  }, [content]);

  return (
    <div
      ref={editorRef}
      className="prose prose-sm max-w-none"
    />
  );
}
