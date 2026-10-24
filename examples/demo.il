/* Synthetic SKILL source for exercising the server. */
procedure(myShape(cell name)
  width = 2
  dbSynthetic(
    cell
    name
    ?width width
    ?purpose "drawing"
  )
)
myShape(nil "demo")
