use http-nu/html *

{|req|
  DIV (
    H1 {class: [bg-green shadow-glow]}
    "Hai from Nushell!"
  ) (
    UL {
      1..3 | each {|n|
        LI $"Item ($n)"
      }
    }
  )
}
