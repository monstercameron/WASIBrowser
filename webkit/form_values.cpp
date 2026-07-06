// form_values.cpp
//
// Implements gdom::Agent::applyFormValue, which sets the programmatic value of
// form controls (input, textarea) in response to an OpSetValue batch op. This
// is a separate translation unit because the HTMLInputElement and
// HTMLTextAreaElement headers pull in a large slice of the WebCore DOM tree;
// isolating them keeps compile times manageable.
//
// Called from dom_agent.cpp (Agent::apply, OpSetValue case) on the web-process
// main thread. Must not be called from any other thread.

#include <WebCore/Element.h>
#include <WebCore/HTMLInputElement.h>
#include <WebCore/HTMLTextAreaElement.h>
#include <WebCore/HTMLSelectElement.h>

#include <wtf/text/WTFString.h>

// This file is compiled as part of the dom_agent.cpp translation unit via
// dom_agent.cpp's implicit link, or as a separate object linked into the bundle.
// Either way, it defines the out-of-line member declared in dom_agent.cpp:
//
//   void gdom::Agent::applyFormValue(WebCore::Element*, String&&);

namespace gdom {

// applyFormValue sets the user-visible value of a form control without
// triggering the full input-event chain (use OpSubscribeEvent for that).
// Dispatches on element type:
//   HTMLInputElement   → setValue(v, DispatchNoEvent)
//   HTMLTextAreaElement → setValue(v, DispatchNoEvent)
//   HTMLSelectElement  → setValue(v)           // selects the matching option
//   anything else      → sets the "value" attribute as a fallback
void Agent::applyFormValue(WebCore::Element* el, String&& value)
{
    if (!el)
        return;

    if (auto* input = dynamicDowncast<WebCore::HTMLInputElement>(el)) {
        // setValue with DispatchNoEvent avoids triggering the oninput handler
        // from the Go side (the app decides when to fire events).
        input->setValue(value, WebCore::TextFieldEventBehavior::DispatchNoEvent);
        return;
    }

    if (auto* textarea = dynamicDowncast<WebCore::HTMLTextAreaElement>(el)) {
        textarea->setValue(value, WebCore::TextFieldEventBehavior::DispatchNoEvent);
        return;
    }

    if (auto* select = dynamicDowncast<WebCore::HTMLSelectElement>(el)) {
        // setValueAsString selects the first <option> whose value matches.
        select->setValue(value);
        return;
    }

    // Fallback: write the "value" attribute on any other element type.
    // This handles custom elements and future form controls.
    el->setAttribute(WebCore::HTMLNames::valueAttr, AtomString(value));
}

} // namespace gdom
