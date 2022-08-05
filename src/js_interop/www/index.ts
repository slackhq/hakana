import * as CodeMirror from 'codemirror';
import * as CodeMirrorLint from '@codemirror/lint';
import { loadWASM } from 'onigasm'
import { add_lint } from './codemirror_lint.js';

import './sass/codemirror.css';
import './sass/codemirror_custom.css';

import { compressUrlSafe, decompressUrlSafe } from './lzma-url.js';

import {
  activateLanguage,
  addGrammar,

  // [ optional | recommended ] Textmate themes in CodeMirror
  addTheme,
  ITextmateThemePlus,
  // [ optional ] Grammar injections
  linkInjections,
} from 'codemirror-textmate';

import { ScannerAndAnalyzer } from "../pkg/js_interop.js";

(async () => {
  await loadWASM(
    // webpack has been configured to resolve `.wasm` files to actual 'paths" as opposed to using the built-in wasm-loader
    // oniguruma is a low-level library and stock wasm-loader isn't equipped with advanced low-level API's to interact with libonig
    require('onigasm/lib/onigasm.wasm').default)

  const grammars = {
    // loading `source.js` as a standalone grammar and as dependency of `text.html.basic` 
    'source.hack': {
      /**
       * This the most resource efficient way to load grammars as of yet
       */
      loader: () => import('./tm/grammars/hack.tmLanguage.json'),

      /**
       * Language ID is only necessary for languages you want to use as CodeMirror mode (eg: cm.setOption('mode', 'javascript'))
       * To do that, we use `activatelanguage`, which will link one scope name to a language ID (also known as "mode")
       * 
       * Grammar dependencies don't need to be "activated", just "adding/registering" them is enough (using `addGrammar`)
       */
      language: 'hack',

      /**
       * Third parameter accepted by `activateLanguage` to specify language loading priority
       * Loading priority can be 'now' | 'asap' | 'defer' (default)
       * 
       *  - [HIGH] 'now' will cause the language (and it's grammars) to load/compile right away (most likely in the next event loop)
       *  - [MED]  'asap' is like 'now' but will use `requestIdleCallback` if available (fallbacks to `setTimeout`, 10 seconds).
       *  - [LOW]  'defer' will only do registeration and loading/compiling is deferred until needed (⚠ WILL CAUSE FOUC IN CODEMIRROR) (DEFAULT)
       */
      priority: 'now'
    },
  }

  // To avoid FOUC, await for high priority languages to get ready (loading/compiling takes time, and it's an async process for which CM won't wait)
  await Promise.all(Object.keys(grammars).map(async scopeName => {
    const { loader, language, priority } = grammars[scopeName]

    addGrammar(scopeName, loader)

    if (language) {
      const prom = activateLanguage(scopeName, language, priority)

      // We must "wait" for high priority languages to load/compile before we render editor to avoid FOUC (Flash of Unstyled Content)
      if (priority === 'now') {
        await prom
      }

      // 'asap' although "awaitable", is a medium priority, and doesn't need to be waited for
      // 'defer' doesn't support awaiting at all
      return
    }
  }))

  const wasm = await require('../pkg/js_interop');

  const scanner_analyzer = new ScannerAndAnalyzer();

  function escapeHtml(snippet: string): string {
    return snippet.replace(/[\u00A0-\u9999<>\&]/gim, function (i) {
      return '&#' + i.charCodeAt(0) + ';';
    });
  }

  const fetchAnnotations = function (code: string, callback, options, cm) {
    let results = scanner_analyzer.get_results(code);
    let response = JSON.parse(results);

    if ('results' in response) {
      var hakana_header = 'Hakana output: <br><br>'

      if (response.results.length === 0) {
        document.getElementById('hakana_output').innerHTML = hakana_header + 'No issues!';
        callback([]);
      }
      else {
        var text = response.results.map(
          function (issue) {
            let message = (issue.severity === 'error' ? 'ERROR' : 'INFO') + ': '
              + '<a href="' + issue.link + '" target="_blank">' + issue.type + '</a> - ' + issue.line_from + ':'
              + issue.column_from + ' - ' + escapeHtml(issue.message);

            if (issue.other_references) {
              message += "<br><br>"
                + issue.other_references.map(
                  function (reference) {
                    let snippet = reference.snippet;

                    let selection_start = reference.from - reference.snippet_from;
                    let selection_end = reference.to - reference.snippet_from;

                    snippet = escapeHtml(snippet.substring(0, selection_start))
                      + "<span style='color: black;background-color:#ddd;'>"
                      + escapeHtml(snippet.substring(selection_start, selection_end))
                      + "</span>" + escapeHtml(snippet.substring(selection_end));
                    return '&nbsp;&nbsp;' + reference.label
                      + ' - ' + reference.line_from
                      + ':' + reference.column_from
                      + '<br>&nbsp;&nbsp;&nbsp;&nbsp;' + snippet;
                  }
                ).join("<br><br>");
            }

            if (issue.taint_trace) {
              message += "<br><br>"
                + issue.taint_trace.map(
                  function (reference) {
                    if (!("snippet" in reference)) {
                      return '&nbsp;&nbsp;' + reference.label;
                    }

                    let snippet = reference.snippet;

                    let selection_start = reference.from - reference.snippet_from;
                    let selection_end = reference.to - reference.snippet_from;

                    snippet = escapeHtml(snippet.substring(0, selection_start))
                      + "<span style='color: black;background-color:#ddd;'>"
                      + escapeHtml(snippet.substring(selection_start, selection_end))
                      + "</span>" + escapeHtml(snippet.substring(selection_end));
                    return '&nbsp;&nbsp;' + reference.label
                      + ' - ' + reference.line_from
                      + ':' + reference.column_from
                      + '<br>&nbsp;&nbsp;&nbsp;&nbsp;' + snippet;
                  }
                ).join("<br><br>");
            }

            return message;
          }
        );

        document.getElementById('hakana_output').innerHTML = hakana_header + text.join('<br><br>') + '<br>&nbsp;';

        callback(
          response.results.map(
            function (issue) {
              return {
                severity: issue.severity === 'error' ? 'error' : 'warning',
                message: issue.message,
                from: cm.posFromIndex(issue.from),
                to: cm.posFromIndex(issue.to)
              };
            }
          )
        );
      }
    }
    else if ('error' in response) {
      var error_type = response.error.type === 'parser_error' ? 'Parser' : 'Internal Hakana';
      document.getElementById('hakana_output').innerText = 'Hakana runner output: \n\n'
        + error_type + ' error on line ' + response.error.line_from + ' - '
        + response.error.message;

      callback({
        message: response.error.message,
        severity: 'error',
        from: cm.posFromIndex(response.error.from),
        to: cm.posFromIndex(response.error.to),
      });
    };
  };

  add_lint(CodeMirror);

  let textarea = document.getElementById('hack_code') as HTMLTextAreaElement;

  if (window.location.hash) {
    textarea.value = decompressUrlSafe(window.location.hash);
  } else {
    textarea.value = `function foo(): void {
    $a = 1;
    $b = 2;
    $arr = vec[$a, $b];
    echo $arr[0];
}`;
  }

  const editor = CodeMirror.fromTextArea(textarea, {
    lineNumbers: true,
    matchBrackets: true,
    lineSeparator: "\n",
    mode: 'hack',
    inputStyle: 'contenteditable',
    indentWithTabs: false,
    indentUnit: 4,
    theme: 'default',
    lint: {
      getAnnotations: fetchAnnotations,
      async: true,
      delay: 50,
    }
  });

  editor.on('change', function () {
    window.location.hash = "";
  });

  document.querySelector('#get_link_button').addEventListener('click', function (e) {
    let hashed = compressUrlSafe(editor.getValue());
    e.preventDefault();
    window.location.hash = hashed;
    return false;
  });
})();
