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

import { ScannerAndAnalyzer } from "../pkg/js_interop";

(async () => {
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
    mode: '',
    inputStyle: 'contenteditable',
    indentWithTabs: false,
    indentUnit: 4,
    theme: 'default',
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

  function escapeHtml(snippet: string): string {
    return snippet.replace(/[\u00A0-\u9999<>\&]/gim, function (i) {
      return '&#' + i.charCodeAt(0) + ';';
    });
  }

  setTimeout(async () => {
    // webpack has been configured to resolve `.wasm` files to actual 'paths" as opposed to using the built-in wasm-loader
    // oniguruma is a low-level library and stock wasm-loader isn't equipped with advanced low-level API's to interact with libonig
    await loadWASM(require('onigasm/lib/onigasm.wasm').default);
    let languageId = 'source.hack';
    addGrammar(languageId, () => import('./tm/grammars/hack.tmLanguage.json'));
    await activateLanguage(languageId, 'hack', 'asap');
    editor.setOption('mode', 'hack');

    setTimeout(() => {
      const scanner_analyzer = new ScannerAndAnalyzer();

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
                  + issue.type + ' - ' + issue.line_from + ':'
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

      editor.setOption('lint', {
        getAnnotations: fetchAnnotations,
        async: true,
        delay: 50,
      });
    });
  }, 0);
})();
