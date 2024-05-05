Output assets/demo.gif

Set Theme "Aardvark Blue"
Set Padding 10
Set WindowBar Colorful
Set BorderRadius 10
Set FontSize 22
Set Width 1200
Set Height 720
Set TypingSpeed 80ms

# Setup
Hide
Type "rm -f src/foo.rs"
Enter
Ctrl+l
Show

Sleep 2s

Type "mtl local build --progress"
Enter
Sleep 2s

Type "mtl cat-object HEAD"
Enter
Sleep 1s

Type "mtl cat-object HEAD:src"
Enter
Sleep 1s

Type "mtl ref save saved-head"
Enter
Sleep 1s

Type "touch src/foo.rs"
Enter
Sleep 1s

Type "mtl local build"
Enter
Sleep 1s

Type "mtl diff saved-head HEAD"
Enter
Sleep 5s
