# basic.nu - A basic HTTP server example for http-nu
#
# Run with: cat examples/basic.nu | http-nu :3001 -

{|req|
  match $req.path {
    # Home page
    "/" => {
      .response { headers: { "Content-Type": "text/html" } }
      "<html><body>
        <h1>http-nu demo</h1>
        <ul>
          <li><a href='/hello'>Hello World</a></li>
          <li><a href='/json'>JSON Example</a></li>
          <li><a href='/echo'>POST Echo</a></li>
          <li><a href='/time'>Current Time</a></li>
          <li><a href='/info'>Request Info</a></li>
        </ul>
      </body></html>"
    }

    # Hello world example
    "/hello" => {
      "Hello, World!"
    }

    # JSON response example
    "/json" => {
      {
        message: "This is JSON",
        timestamp: (date now | into int),
        server: "http-nu"
      }
    }

    # Echo POST data
    "/echo" => {
      if $req.method == "POST" {
        # Return the request body
        $in
      } else {
        .response { headers: { "Content-Type": "text/html" } }
        "<html><body>
          <h1>Echo Service</h1>
          <p>Send a POST request to this URL to echo the body.</p>
          <form method='post'>
            <textarea name='data'></textarea>
            <br>
            <button type='submit'>Submit</button>
          </form>
        </body></html>"
      }
    }

    # Time stream example
    "/time" => {
      .response { headers: { "Content-Type": "text/plain" } }
      generate {|_|
        sleep 1sec
        {out: $"Current time: (date now | format date '%Y-%m-%d %H:%M:%S')\n" next: true}
      } true
    }

    # Show request info
    "/info" => {
      $req
    }

    # 404 for everything else
    _ => {
      .response { status: 404 }
      "404 - Page not found"
    }
  }
}
