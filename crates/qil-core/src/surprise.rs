//! "Surprise me": a random idea. Half the time it combines parts
//! (style + subject + place + light), half the time it uses a curated full prompt.
//! Prompts are in English because the model follows English best.

use rand::seq::SliceRandom;
use rand::Rng;
use serde::Serialize;

const SUBJECTS: &[&str] = &[
    "an elderly clockmaker repairing a pocket watch",
    "a red fox curled up asleep",
    "a vintage motorcycle covered in dew",
    "a street musician playing bandoneon",
    "a greenhouse full of carnivorous plants",
    "a lighthouse keeper's cluttered desk",
    "a jellyfish drifting through a flooded subway car",
    "a tiny robot tending a bonsai tree",
    "a cat wearing a knitted scarf on a windowsill",
    "a skateboarder mid-air over an empty pool",
    "an astronaut sitting on a porch drinking mate",
    "a paper boat sailing down a rainy gutter",
    "a whale made of clouds swimming over a city",
    "a tango couple frozen mid-step",
    "a chef plating a dessert with tweezers",
    "an abandoned amusement park carousel",
];
const PLACES: &[&str] = &[
    "in a narrow cobblestone alley in San Telmo",
    "on a salt flat at the edge of the world",
    "inside a brutalist concrete library",
    "on a rooftop in Tokyo after rain",
    "in a misty Patagonian forest",
    "in a 1970s living room with wood paneling",
    "on a tiny floating island in the sky",
    "at a desert gas station at night",
    "inside an overgrown glasshouse",
    "on a snowy mountain pass",
    "in a neon-lit night market",
    "on the shore of a black-sand beach",
];
const STYLES: &[&str] = &[
    "35mm film photograph, Kodak Portra 400 grain",
    "cinematic still, anamorphic lens, shallow depth of field",
    "editorial fashion photograph",
    "risograph print with a limited palette of teal, orange and cream",
    "detailed ligne claire comic illustration",
    "soft watercolor and ink illustration",
    "claymation stop-motion still with visible fingerprints",
    "isometric low-poly 3D render",
    "1950s screen-printed travel poster",
    "Dutch golden age oil painting",
    "high-end product photograph",
    "ukiyo-e woodblock print",
    "feature animation 3D still with expressive characters",
    "architectural photograph, tilt-shift lens",
];
const LIGHTS: &[&str] = &[
    "golden hour backlight",
    "harsh midday sun with deep shadows",
    "soft overcast light",
    "blue hour with warm window glow",
    "single candle light",
    "neon reflections on wet surfaces",
    "volumetric fog and god rays",
    "direct on-camera flash",
    "moonlight and lantern light",
];
const TWISTS: &[&str] = &[
    "in an unexpected surreal situation",
    "made entirely of glass",
    "at a miniature scale on a kitchen table",
    "during a lively street festival",
    "in a quiet, melancholic moment",
    "reimagined far in the future",
    "as the hero of an old adventure story",
];
const CURATED: &[&str] = &[
    "A cozy bookstore built inside the hull of an upturned wooden ship, warm lamps, spiral staircase, a cat asleep on a stack of atlases, cinematic wide shot.",
    "Macro photograph of a dewdrop on a spider web reflecting an entire sunrise over the Andes, extremely detailed.",
    "A vintage Argentine colectivo bus painted with fileteado porteño ornaments, parked on a sunny Buenos Aires street, 35mm film.",
    "A giant sleeping stone dragon covered in moss forming a mountain range, tiny hikers walking along its spine, aerial view, morning mist.",
    "Minimalist poster: a single red umbrella in a vast white snowfield, long shadow, lots of negative space, bold serif title \"SILENCIO\" at the bottom.",
    "A steampunk hummingbird made of brass gears and stained glass hovering near a real orchid, studio product lighting, black background.",
    "Portrait of a grandmother laughing in her kitchen while kneading dough for empanadas, flour in the air, soft window light, documentary photography.",
    "An underwater library where fish swim between shelves, light beams from the surface, books floating open, fantasy illustration.",
    "Isometric cutaway of a tiny three-story Buenos Aires apartment building showing each family's life, detailed, warm palette.",
    "A lone astronaut fishing on the edge of a crater lake on Mars, pink sky, two small moons, cinematic.",
    "Retro 1980s sci-fi paperback cover of a city growing on the back of a giant turtle, painted, with the title \"THE STEEL TORTOISE\".",
    "A glass teapot containing a miniature thunderstorm, lightning inside, on a wooden table, product photograph.",
];
pub const ASPECTS: &[&str] = &["1:1", "4:3", "3:4", "3:2", "2:3", "16:9", "9:16"];

#[derive(Debug, Clone, Serialize)]
pub struct Idea {
    pub prompt: String,
    pub aspect: String,
}

pub fn idea(theme: Option<&str>) -> Idea {
    let mut r = rand::thread_rng();
    let pick = |xs: &[&'static str], r: &mut rand::rngs::ThreadRng| *xs.choose(r).unwrap();
    let theme = theme.map(str::trim).filter(|t| !t.is_empty());
    let prompt = match theme {
        None if r.gen_bool(0.5) => pick(CURATED, &mut r).to_string(),
        _ => {
            let subject = match theme {
                Some(t) => format!("{t} {}", pick(TWISTS, &mut r)),
                None => pick(SUBJECTS, &mut r).to_string(),
            };
            let s = format!(
                "{} of {subject} {}, {}. Rich detail, strong composition.",
                pick(STYLES, &mut r),
                pick(PLACES, &mut r),
                pick(LIGHTS, &mut r)
            );
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or(s)
        }
    };
    Idea {
        prompt,
        aspect: pick(ASPECTS, &mut r).to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn theme_is_used() {
        for _ in 0..20 {
            let i = super::idea(Some("capybaras"));
            assert!(i.prompt.contains("capybaras"));
            assert!(super::ASPECTS.contains(&i.aspect.as_str()));
        }
    }
}
