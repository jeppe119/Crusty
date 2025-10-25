# 🎯 RUST TYPE PRONUNCIATION GUIDE

A beginner-friendly guide to reading and understanding Rust syntax.

---

## **Basic Types**

### Strings
```rust
String          → "a text string (owned)" - you own it, can modify it
                → Example: let title = String::from("Hello");
                → Example: let name = "Rust".to_string();

&str            → "a string slice (borrowed/reference)" - read-only text
                → Example: let greeting: &str = "Hello, world!";
                → Example: fn print_text(text: &str) { println!("{}", text); }
```

**When to use:**
- `String` - When you need to own/modify text (like storing a song title)
- `&str` - When you just need to read text (like function parameters)

### Numbers
```rust
i32             → "a 32-bit INTEGER (signed)" - whole numbers from -2 billion to +2 billion
                → Example: let age: i32 = 25;
                → Example: let temperature: i32 = -10;
                → Used for: counting, indexes, IDs

u64             → "an UNSIGNED 64-bit number" - only positive numbers, 0 to 18 quintillion
                → Example: let duration: u64 = 180; // 180 seconds
                → Example: let file_size: u64 = 5_000_000; // 5 million bytes
                → Used for: sizes, durations, timestamps (can't be negative!)

i64             → "a 64-bit INTEGER (signed)" - whole numbers (can be negative)
                → Example: let balance: i64 = -500;
                → Range: -9 quintillion to +9 quintillion

f64             → "a 64-bit FLOAT" - decimal numbers
                → Example: let volume: f64 = 0.75; // 75% volume
                → Example: let position: f64 = 3.14159;
                → Used for: percentages, precise measurements

usize           → "unsigned SIZE" - positive integers, size matches your computer (32 or 64 bit)
                → Example: let index: usize = 5;
                → Example: let length = vec.len(); // returns usize
                → Used for: array indexes, collection sizes
```

**Number Prefixes:**
- `i` = signed (can be negative): i8, i16, i32, i64, i128
- `u` = unsigned (only positive): u8, u16, u32, u64, u128
- Number after = how many bits (bigger = more range)

### Boolean
```rust
bool            → "true or false" - only two possible values
                → Example: let is_playing: bool = true;
                → Example: let is_paused = false;
                → Example: if is_playing { /* do something */ }
```

---

## **Collections**

### Vec - Dynamic Array/List
```rust
Vec<Track>           → "a vector OF tracks" (dynamic array/list)
                     → Example: let mut songs = Vec::new();
                     → Example: let numbers = vec![1, 2, 3, 4, 5];
                     → Example: songs.push(track);  // add to end
                     → Example: songs.pop();        // remove from end

// Real example from your project:
let mut history: Vec<Track> = Vec::new();
history.push(track1);
history.push(track2);
let last = history.pop();  // removes and returns track2
```

**When to use:** When you need a list that grows/shrinks, mainly add/remove from the END

### VecDeque - Double-Ended Queue
```rust
VecDeque<Track>      → "a double-ended queue OF tracks" (can add/remove from both ends!)
                     → Example: let mut queue = VecDeque::new();
                     → Example: queue.push_back(track);   // add to back
                     → Example: queue.push_front(track);  // add to front
                     → Example: queue.pop_front();        // remove from front
                     → Example: queue.pop_back();         // remove from back

// Real example from your project:
let mut tracks: VecDeque<Track> = VecDeque::new();
tracks.push_back(track1);   // [track1]
tracks.push_back(track2);   // [track1, track2]
let next = tracks.pop_front();  // removes track1, leaves [track2]
```

**When to use:** When you need a queue/playlist where you add to one end and remove from the other

### HashMap - Key-Value Storage
```rust
HashMap<String, i32> → "a hash map FROM string TO integer" (key-value pairs like a dictionary)
                     → Example: let mut scores = HashMap::new();
                     → Example: scores.insert("Alice".to_string(), 100);
                     → Example: let score = scores.get("Alice");  // returns Some(100)

// Real example:
use std::collections::HashMap;
let mut config: HashMap<String, String> = HashMap::new();
config.insert("theme".to_string(), "dark".to_string());
if let Some(theme) = config.get("theme") {
    println!("Theme is: {}", theme);  // prints "Theme is: dark"
}
```

**When to use:** When you need to look up values by a key (like a phone book: name → number)

### Visual Comparison

```
Vec:       [track1] [track2] [track3]
           ↑                        ↑
           Can only easily work    Can easily add/remove here
           with this end           (push/pop)

VecDeque:  [track1] [track2] [track3]
           ↑                        ↑
           Can add/remove          Can add/remove
           here too!              here too!
           (push_front/pop_front) (push_back/pop_back)

HashMap:   "song1" → Track { title: "Rust Music", ... }
           "song2" → Track { title: "Code Beats", ... }
           ↑         ↑
           Key       Value (fast lookup by key!)
```

---

## **Option & Result**

### Option - Maybe Something, Maybe Nothing
```rust
Option<Track>        → "maybe a track, maybe nothing"
                     → Can be: Some(track) or None

// Creating Options:
let has_track: Option<Track> = Some(track);  // has a value
let no_track: Option<Track> = None;          // empty

// Real example from your project:
pub struct Queue {
    current_track: Option<Track>,  // might be playing a track, might not
}

// Using Options:
match self.current_track {
    Some(track) => println!("Playing: {}", track.title),
    None => println!("Nothing playing"),
}

// Or with if let:
if let Some(track) = self.current_track {
    println!("Playing: {}", track.title);
}

// Getting the value (dangerous - panics if None!):
let track = option.unwrap();  // DON'T USE unless you're 100% sure it's Some

// Getting the value safely:
let track = option.unwrap_or(default_track);  // use default if None
```

**When to use:** When something might not exist (like "current song" when nothing is playing)

### Result - Success or Error
```rust
Result<Vec<Track>, Error>
                     → "either a vector of tracks OR an error"
                     → Can be: Ok(vec) or Err(error)

// Creating Results:
fn search_youtube(query: &str) -> Result<Vec<Track>, String> {
    if query.is_empty() {
        return Err("Query cannot be empty".to_string());  // error case
    }
    let tracks = vec![/* ... */];
    Ok(tracks)  // success case
}

// Using Results:
match search_youtube("rust music") {
    Ok(tracks) => println!("Found {} tracks", tracks.len()),
    Err(e) => println!("Error: {}", e),
}

// Or use the ? operator (if error, return early):
let tracks = search_youtube("rust music")?;  // if Err, return the error
println!("Found {} tracks", tracks.len());    // only runs if Ok

// Real example from your project:
pub async fn get_audio_url(&self, video_url: &str)
    -> Result<String, Box<dyn std::error::Error>>
{
    // Try to get audio URL
    // If successful: return Ok(url)
    // If fails: return Err(error)
}
```

**When to use:** When an operation might fail (network requests, file I/O, parsing, etc.)

### Option vs Result Quick Guide

```
Option<T>                    Result<T, E>
---------                    ------------
"Maybe has a value"         "Success or failure"
Some(value) or None         Ok(value) or Err(error)

Examples:                    Examples:
- Current song               - Network request
- User input                 - File read/write
- Search in list             - Parsing JSON
- Next item in queue         - YouTube API call
```

---

## **References & Borrowing**

```rust
&Track              → "a reference TO a track" (borrowed, read-only)
&mut Track          → "a mutable reference TO a track" (borrowed, can modify)
&self               → "borrow self" (read-only access to struct)
&mut self           → "borrow self mutably" (can modify struct fields)
```

**Key Rules:**
- You can have many `&` (read-only) references
- You can have only ONE `&mut` (mutable) reference at a time
- Can't have `&` and `&mut` at the same time

---

## **Pointers & Smart Pointers**

```rust
Box<Track>          → "a box containing a track" (heap-allocated, single owner)
Arc<Track>          → "an atomic reference counted track" (shared ownership, thread-safe)
Mutex<Track>        → "a mutex protecting a track" (thread-safe exclusive access)
Rc<Track>           → "a reference counted track" (shared ownership, NOT thread-safe)
```

**When to use:**
- `Box<T>` - When you need heap allocation or trait objects
- `Arc<T>` - When multiple threads need to share read-only data
- `Mutex<T>` - When multiple threads need to modify shared data
- `Rc<T>` - Like Arc but for single-threaded code

---

## **Function Signatures - Read Left to Right**

```rust
fn add(&mut self, track: Track)
```
→ "function 'add' that borrows self mutably, takes a track"

```rust
fn next(&mut self) -> Option<Track>
```
→ "function 'next' that borrows self mutably, returns maybe a track"

```rust
async fn search(&self, query: &str) -> Result<Vec<VideoInfo>, Error>
```
→ "async function 'search' that borrows self, takes a string reference,
   returns either a vector of video info OR an error"

```rust
pub fn get_current(&self) -> Option<&Track>
```
→ "public function 'get_current' that borrows self, returns maybe a reference to a track"

---

## **Special Syntax**

```rust
::                  → "path separator" (like / in file paths)
                    → Vec::new() = "call 'new' from Vec type"
                    → std::collections::HashMap = "HashMap from std's collections module"

<T>                 → "generic type T"
                    → Vec<Track> = "Vec of whatever type, here Track"
                    → Option<T> = "Option can hold any type T"

dyn Trait           → "dynamic trait" (any type that implements this trait)
                    → Box<dyn Error> = "a boxed value of any type that implements Error"

?                   → "if error, return early with that error"
                    → let x = function()?; = "call function, if error bail out"

.                   → "call method or access field"
                    → track.title = "access title field"
                    → vec.push(item) = "call push method"

!                   → "macro call" (code that generates code)
                    → println!() = "call println macro"
                    → vec![] = "call vec macro to create a vector"
```

---

## **Pattern Matching Syntax**

```rust
if let Some(track) = self.current_track.take()
```
→ "If current_track has something inside, take it out (leaving None behind) and call it 'track'"

```rust
match result {
    Ok(value) => { /* use value */ },
    Err(e) => { /* handle error */ },
}
```
→ "Check if result is Ok or Err, do different things for each case"

```rust
for track in self.tracks.iter() {
    // ...
}
```
→ "For each track in the tracks collection, do something"

---

## **Ownership & Methods**

```rust
.take()             → "take the value out, leave None behind"
                    → Works on Option<T>

.clone()            → "make a duplicate copy of this value"
                    → Creates a new owned value

.iter()             → "iterate over references" (borrows items)
.iter_mut()         → "iterate over mutable references" (can modify items)
.into_iter()        → "iterate by taking ownership" (consumes collection)

.push()             → "add to the end"
.pop()              → "remove from the end and return it"
.push_front()       → "add to the front" (VecDeque only)
.pop_front()        → "remove from the front" (VecDeque only)

.clear()            → "remove all items, make empty"
.len()              → "get the number of items"
.is_empty()         → "check if there are zero items"
```

---

## **Common Patterns**

### Creating Empty Collections
```rust
Vec::new()          → "create a new empty vector"
VecDeque::new()     → "create a new empty deque"
HashMap::new()      → "create a new empty hash map"
```

### Handling Options
```rust
if let Some(value) = option {
    // use value
}

match option {
    Some(value) => { /* use value */ },
    None => { /* handle empty case */ },
}

option.unwrap()     → "give me the value or panic if None" (dangerous!)
option.unwrap_or(default) → "give me the value or use default if None"
```

### Handling Results
```rust
let value = result?;  → "if error, return early; if ok, unwrap value"

match result {
    Ok(value) => { /* use value */ },
    Err(e) => { /* handle error */ },
}

result.unwrap()     → "give me the value or panic if Err" (dangerous!)
```

---

## **🎬 REAL EXAMPLES FROM YOUR PROJECT**

### Example 1: Queue's next() function
```rust
pub fn next(&mut self) -> Option<Track>
```
**Plain English:** "Public function called 'next' that can modify the queue and returns maybe a track"

### Example 2: Taking current track
```rust
if let Some(track) = self.current_track.take() {
    self.history.push(track);
}
```
**Plain English:** "If there's a current track, take it out (leaving None), and push it into history"

### Example 3: Search function
```rust
pub async fn search(&self, query: &str, max_results: usize)
    -> Result<Vec<VideoInfo>, Box<dyn std::error::Error>>
```
**Plain English:** "Public async function called 'search' that borrows self, takes a string reference and a number, and returns either a vector of video info or any kind of error in a box"

### Example 4: Track struct
```rust
#[derive(Debug, Clone)]
pub struct Track {
    pub video_id: String,
    pub title: String,
    pub duration: u64,
}
```
**Plain English:** "A public struct called Track that can be printed for debugging and can be cloned. It has public fields: video_id (owned string), title (owned string), and duration (unsigned 64-bit number)"

---

## **💡 TIPS FOR READING RUST CODE**

1. **Start with the function signature** - understand what it takes and returns
2. **Look for `&` and `&mut`** - tells you about borrowing and modification
3. **Check `Option` and `Result`** - tells you what can be missing or fail
4. **Follow the ownership** - who owns the data? Is it borrowed? Moved?
5. **Read `::` as "from"** - `Vec::new()` = "new from Vec"
6. **Read `<>` as "of"** - `Vec<Track>` = "Vec of Track"

---

## **📚 PRACTICE EXERCISES**

Try reading these in plain English:

```rust
pub fn add_multiple(&mut self, tracks: Vec<Track>)
```

```rust
pub fn get_current(&self) -> Option<&Track>
```

```rust
pub async fn get_video_info(&self, video_url: &str)
    -> Result<VideoInfo, Box<dyn std::error::Error>>
```

```rust
let mut queue = VecDeque::<Track>::new();
```

**Answers:**
1. "Public function 'add_multiple' that mutably borrows self and takes a vector of tracks"
2. "Public function 'get_current' that borrows self and returns maybe a reference to a track"
3. "Public async function 'get_video_info' that borrows self, takes a string reference, and returns either video info or any error type in a box"
4. "Create a mutable variable called queue, which is a new empty double-ended queue of tracks"

---

## **🔗 USEFUL RESOURCES**

- [The Rust Book](https://doc.rust-lang.org/book/) - Official guide
- [Rust By Example](https://doc.rust-lang.org/rust-by-example/) - Learn by doing
- [Rustlings](https://github.com/rust-lang/rustlings) - Small exercises

---

**Good luck with your Rust journey! 🦀**
