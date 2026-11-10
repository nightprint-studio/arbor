//! What differs between one product's Imber schedules and another's. Two implementations (Corvus, Bennu) is
//! what makes this a trait: the tools, the words and the handlers' logic are written once against it.

use arbor_scheduler::prelude::JobAction;
use serde_json::{json, Value};

pub trait Flavour {
    /// The product's action enum.
    type Action: JobAction + Send;

    /// The app that has to be open for a schedule to fire (`Corvus`, `Bennu`).
    fn app(&self) -> &'static str;

    /// What a schedule is *for*, in the tool descriptions: `repository`, `project`.
    fn unit(&self) -> &'static str;

    /// The action words the model may choose from (`awake`, `prompt`, …).
    fn actions(&self) -> &'static [&'static str];

    /// The mode words a tool may ask for.
    fn modes(&self) -> &'static [&'static str];

    /// A sentence per action, for the `schedule_create` description.
    fn actions_doc(&self) -> &'static str;

    /// What the modes mean for an unattended run, for the `schedule_create` description.
    fn modes_doc(&self) -> &'static str;

    /// Further string properties `schedule_create` takes (`remote` for a fetch), as a JSON object of schemas.
    fn extra_create_properties(&self) -> Value {
        json!({})
    }

    /// The action a `schedule_create` call asks for. `conversation_id` is the conversation making the call
    /// (what `awake` wakes).
    fn action_from_input(&self, word: Option<&str>, prompt: Option<String>, input: &Value, conversation_id: &str) -> Result<Self::Action, String>;

    /// The action with a new prompt, when it has one; unchanged otherwise.
    fn with_prompt(&self, current: &Self::Action, prompt: String) -> Self::Action;

    /// Any text a caller sent as a mode, as one of this product's words (the cautious one when unknown).
    fn mode_word(&self, raw: &str) -> String;

    /// The mode a job created in a conversation of `conversation_mode` gets: what was asked, but no freer than
    /// the conversation.
    fn mode_for(&self, requested: Option<&str>, conversation_mode: &str) -> String;

    /// Said after a create when the mode asked for was lowered.
    fn lowered_note(&self) -> &'static str;
}
