- allow user to let nodes execute without manually stepping
- allow optional comma between instruction arguments
- add ANY and LAST ports
- add hints for which keys spawn which nodes when the highlighted node coordinate doesn't contain a node
- bug: currently, using `ctrl + O` or `ctrl + S` causes the update/render loop to block on the file select dialogue. This causes the key repeat checker to realize enough time has passed between O or S being pressed that it can repeat the keypress, causing the dialogue to open again immediately