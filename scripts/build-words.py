"""Builds the word lists in src/words/ for each word length.

For length N it writes:
  src/words/allowed-N.txt   every word accepted as a step
  src/words/familiar-N.txt  words used to work out par, hints and the shown
                            route, so they never rely on obscure words
  src/words/common-N.txt    words a puzzle may start or end on

Sources:
  - SCOWL (Spell Checker Oriented Word Lists) by Kevin Atkinson, the lists
    behind Debian's /usr/share/dict/words. English, American and British
    words up to size 60 are used. SCOWL keeps proper nouns in separate lists,
    so its word lists are lowercase dictionary words. See
    src/words/SCOWL-Copyright.txt.
  - The `wordfreq` package, used only to rank words by how common they are.
  - /usr/share/dict/propernames (macOS, BSDs), if present, to keep first names
    out of puzzles.

Usage:
    pip install wordfreq
    # download and unpack https://downloads.sourceforge.net/project/wordlist/SCOWL/2020.12.07/scowl-2020.12.07.tar.gz
    python scripts/build-words.py path/to/scowl-2020.12.07
"""

import shutil
import sys
from pathlib import Path

from wordfreq import zipf_frequency

LENGTHS = (4, 5)
MAX_LEVEL_ALLOWED = 60  # SCOWL size: 60 is the usual spell-checker list
MAX_LEVEL_FAMILIAR = 55
MAX_LEVEL_COMMON = 50
MIN_ZIPF_FAMILIAR = 3.3
MIN_ZIPF_COMMON = 3.5
MIN_ZIPF_BASE = 3.0  # "cats" counts as a plural because "cat" is a common word

# Never accepted: words whose main modern use is as a slur
SLURS = {
    "coon", "coons", "dago", "dyke", "fagot", "fags", "gook", "gooks", "jap", "japs",
    "kike", "kikes", "nigga", "paki", "spic", "spics", "squaw", "wetback", "wops", "chink",
    "negro",
}

# Valid steps, but never a puzzle's start or end: crude or sensitive words
EXCLUDE = SLURS | {
    "anal", "arse", "boob", "boobs", "butt", "cock", "crap", "damn", "dick", "dildo",
    "dung", "fart", "fuck", "hell", "homo", "jism", "jizz", "kill", "nazi", "piss",
    "poop", "porn", "rape", "scum", "sexy", "shit", "slut", "tits", "turd", "twat",
    "whore", "bitch", "boner", "booty", "feces", "horny", "penis", "porno", "pubic",
    "pussy", "queer", "semen", "sperm", "urine", "vomit", "bowel", "groin", "gypsy",
    "nude", "cunt", "wank", "lynch", "pimp", "drug", "drugs", "dead", "death", "slay",
    "suicide", "gore", "corpse", "jihad", "kinky", "butch", "milf",
}

# Never a start or end: names, places, brands, abbreviations and slang that
# the dictionary also lists in lowercase, and words unsuited to a casual game
NOT_ENDPOINTS = {
    "alec", "bach", "bali", "baba", "bong", "brit", "cain", "carr", "cass", "chad",
    "coca", "coke", "corp", "dong", "dont", "dope", "hong", "howe", "holt", "indy",
    "jake", "joey", "kang", "kung", "lite", "ling", "luna", "lucy", "mack", "mali",
    "maya", "ming", "mona", "nash", "ness", "noel", "otto", "para", "prep", "prof",
    "rita", "rory", "sept", "spec", "suck", "tara", "tate", "temp", "tory", "vera",
    "wang", "wong", "wont", "yale", "york", "yuan", "alma", "beck", "nana", "june",
    "arent", "didnt", "wasnt", "bowie", "brock", "burke", "chang", "chevy", "chile",
    "china", "clint", "clive", "costa", "covid", "dover", "hanna", "hogan", "homer",
    "honda", "irene", "japan", "jenna", "jenny", "malik", "monte", "nicky", "porto",
    "riley", "ryder", "sammy", "satan", "silva", "singh", "swiss", "tibet", "walsh",
    "weber", "willy", "yahoo", "benny", "dutch", "welsh", "trump", "slave", "idiot",
    "moron", "prick", "fetus", "obese", "booze", "vodka", "drunk", "dying", "paddy",
    "dunno", "gimme", "gonna", "gotta", "kinda", "wanna", "multi", "intel", "hicks",
    "bible", "wales", "thru", "info", "apps", "stats", "mama", "oops", "sans", "feet",
    "women",
}

# Never in par routes or hints: names, slang, abbreviations and archaic words
# that are frequent enough to pass the familiar threshold
NOT_FAMILIAR = {
    "abbas", "aint", "alba", "asses", "cams", "coco", "demi", "dyer", "gifs", "kemp",
    "mins", "muir", "patel", "pong", "prob", "refs", "shes", "sucks", "thine", "torah",
    "twain", "viva", "ames", "burt", "conn", "dodd", "elle", "kidd", "napa", "soho",
    "shea", "mets", "mens", "vols", "sans", "noir", "aires", "blanc", "bosch", "cheng",
    "ching", "cisco", "garth", "hayes", "julio", "kraft", "levin", "paolo", "reese",
    "rowan", "sloan", "wally", "wigan", "whats", "thats", "crore", "mamma", "mater",
    "shalt", "prima", "buffy", "fanny", "gaga", "lulu", "ting", "sims", "sears", "johns",
    "bates", "mecca", "outta",
}

# Real words SCOWL only has in its larger, name-heavy sizes
EXTRA_ALLOWED = {
    "snuck", "melee", "maths", "loch", "chai", "merch", "diss", "ramen", "apnea", "orca",
    "luxe", "tiki",
}

# Words in the system's first-names list that are also ordinary words
NAMES_TO_KEEP = {
    "bill", "case", "chip", "clay", "dawn", "dean", "drew", "duke", "earl", "gale",
    "gene", "gill", "glen", "herb", "jack", "king", "list", "lord", "mark", "nick",
    "norm", "page", "part", "ping", "real", "rich", "root", "skip", "tuna", "vice",
    "wade", "will", "wolf", "carol", "chuck", "cliff", "frank", "grace", "grant",
    "hazel", "holly", "judge", "kitty", "lance", "major", "marsh", "mason", "penny",
    "pilot", "price", "robin", "rogue", "rusty", "sandy", "space", "spike", "think",
    "those", "woody",
}

# Past tenses that look like inflections but aren't ("see" + "d")
NOT_PAST_TENSE = {"seed", "shed", "feed", "need", "deed", "bred", "fled", "sled", "breed"}

ROOT = Path(__file__).resolve().parent.parent


def load_scowl(scowl: Path) -> dict[str, int]:
    """Lowercase SCOWL words mapped to the smallest size level that has them."""
    levels: dict[str, int] = {}
    for path in (scowl / "final").glob("*-words.*"):
        variety, level = path.name.split("-words.")
        if variety not in ("english", "american", "british") or int(level) > MAX_LEVEL_ALLOWED:
            continue
        for word in path.read_text(encoding="latin-1").split():
            if word.isascii() and word.isalpha() and word.islower():
                levels[word] = min(levels.get(word, 99), int(level))
    return levels


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    scowl = Path(sys.argv[1])
    level = load_scowl(scowl)
    for word in EXTRA_ALLOWED:
        level.setdefault(word, MAX_LEVEL_ALLOWED)
    entries = set(level)
    names_path = Path("/usr/share/dict/propernames")
    names = set()
    if names_path.exists():
        names = {w.lower() for w in names_path.read_text().split()} - NAMES_TO_KEEP

    def zipf(w: str) -> float:
        return zipf_frequency(w, "en")

    def is_past_tense(w: str) -> bool:
        """True for "added", "baked", "cried" (but not "seed", "speed")."""
        if not w.endswith("ed") or w in NOT_PAST_TENSE:
            return False
        bases = [w[:-1], w[:-2]]  # bake-d, add-ed
        if w.endswith("ied"):
            bases.append(w[:-3] + "y")  # cry -> cried
        return any(b in entries and zipf(b) >= MIN_ZIPF_BASE for b in bases)

    def is_plural(w: str) -> bool:
        """True for simple plurals of common words ("cats", but not "lens")."""
        if not w.endswith("s") or w.endswith("ss"):
            return False
        bases = [w[:-1]] + ([w[:-2]] if w.endswith("es") else [])
        if w.endswith("ies"):
            bases.append(w[:-3] + "y")  # fly -> flies
        if w.endswith("ves"):
            bases += [w[:-3] + "f", w[:-3] + "fe"]  # wolf -> wolves, wife -> wives
        return any(b in entries and zipf(b) >= MIN_ZIPF_BASE for b in bases)

    out = ROOT / "src" / "words"
    out.mkdir(parents=True, exist_ok=True)
    blocked = EXCLUDE | NOT_ENDPOINTS | NOT_FAMILIAR | names
    for n in LENGTHS:
        allowed = sorted(w for w in entries if len(w) == n and w not in SLURS)
        common = [
            w for w in allowed
            if zipf(w) >= MIN_ZIPF_COMMON and level[w] <= MAX_LEVEL_COMMON
            and w not in EXCLUDE and w not in NOT_ENDPOINTS and w not in names
            and not is_plural(w) and not is_past_tense(w)
        ]
        common_set = set(common)

        def is_familiar(w: str) -> bool:
            if w in common_set:
                return True
            if zipf(w) < MIN_ZIPF_FAMILIAR or level[w] > MAX_LEVEL_FAMILIAR or w in blocked:
                return False
            # Nor any form of an excluded word ("raped", "pimps")
            return not any(w[:-k] in EXCLUDE for k in (1, 2, 3))

        familiar = [w for w in allowed if is_familiar(w)]
        (out / f"allowed-{n}.txt").write_text("\n".join(allowed) + "\n")
        (out / f"familiar-{n}.txt").write_text("\n".join(familiar) + "\n")
        (out / f"common-{n}.txt").write_text("\n".join(common) + "\n")
        print(f"{n} letters: {len(allowed)} allowed, {len(familiar)} familiar, {len(common)} common")

    # SCOWL's licence asks for its copyright notice to accompany the lists
    shutil.copy(scowl / "Copyright", out / "SCOWL-Copyright.txt")


if __name__ == "__main__":
    main()
