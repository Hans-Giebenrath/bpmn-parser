use crate::output::svg::primitives::{
    MESSAGE_FLOW_END_MARKER_WIDTH, MESSAGE_FLOW_START_MARKER_RADIUS, STROKE_WIDTH,
};

use crate::common::graph::*;

// This function is large and annoying to scroll through, so I moved it into its own file.
pub fn defs(stroke: &str) -> String {
    let message_flow_marker_dimension = MESSAGE_FLOW_START_MARKER_RADIUS * 2.;
    let message_flow_marker_ref_y = MESSAGE_FLOW_START_MARKER_RADIUS / 2.;
    format!(
        r##"<defs>
  <style><![CDATA[
  ]]></style>

  <marker id="sequence-flow-head" fill="context-stroke" stroke="none" markerWidth="12" markerHeight="12" refX="6" refY="6" orient="auto-start-reverse" markerUnits="userSpaceOnUse" >
    <path d="M 0 0 L 12 6 L 0 12 Z" stroke-width="{STROKE_WIDTH}" />
  </marker>

  <marker id="data-association-head" markerWidth="9" markerHeight="9" refX="9" refY="4.5" fill="none" stroke="context-stroke" orient="auto-start-reverse" markerUnits="userSpaceOnUse" >
    <path d="M 0 0 L 9 4.5 L 0 9" stroke-width="1.6" />
  </marker>

  <marker id="message-start" refX="0" refY="{message_flow_marker_ref_y}" markerWidth="{message_flow_marker_dimension}" markerHeight="{message_flow_marker_dimension}" stroke="context-stroke" fill="#fff" orient="auto-start-reverse" overflow="visible" markerUnits="userSpaceOnUse" >
    <circle cx="{message_flow_marker_ref_y}" cy="{message_flow_marker_ref_y}" r="{MESSAGE_FLOW_START_MARKER_RADIUS}" stroke-width="{STROKE_WIDTH}" />
  </marker>

  <marker id="message-flow-head" fill="none" stroke="context-stroke" markerWidth="{MESSAGE_FLOW_END_MARKER_WIDTH}" markerHeight="12" refX="0" refY="6" orient="auto-start-reverse" overflow="visible" markerUnits="userSpaceOnUse" >
    <path d="M 0 0 L {MESSAGE_FLOW_END_MARKER_WIDTH} 6 L 0 12 Z" style="stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none" stroke-width="{STROKE_WIDTH}" />
  </marker>

  <symbol id="envelope-outline" viewBox="0 0 20 20" overflow="visible">
      <path
         d="m 3,6 h 14 v 9 H 3 Z"
         fill="none"
         stroke="inherit"
         style="stroke-width:1;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
         />
      <path
         d="m 3,6 7,4 7,-4"
         fill="none"
         stroke="inherit"
         style="stroke-width:1;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
         />
  </symbol>

  <symbol id="envelope-filled" viewBox="0 0 20 20" overflow="visible">
  <path
     style="baseline-shift:baseline;display:inline;overflow:visible;vector-effect:none;stroke-linecap:round;stroke-linejoin:round;enable-background:accumulate;stop-color:inherit"
     d="M 3 5.5 C 2.7238691 5.5000276 2.5000276 5.7238691 2.5 6 L 2.5 15 C 2.5000276 15.276131 2.7238691 15.499972 3 15.5 L 17 15.5 C 17.276131 15.499972 17.499972 15.276131 17.5 15 L 17.5 6 C 17.499972 5.7238691 17.276131 5.5000276 17 5.5 L 3 5.5 z M 16.800781 5.6621094 A 0.5 0.5 0 0 1 17.232422 5.9160156 A 0.5 0.5 0 0 1 17.042969 6.5976562 L 10.246094 10.435547 A 0.50005 0.50005 0 0 1 9.7539062 10.435547 L 2.9570312 6.5976562 A 0.5 0.5 0 0 1 2.7675781 5.9160156 A 0.5 0.5 0 0 1 3.0703125 5.6816406 A 0.5 0.5 0 0 1 3.4492188 5.7265625 L 10 9.4257812 L 16.550781 5.7265625 A 0.5 0.5 0 0 1 16.800781 5.6621094 z " />
  </symbol>

  <!-- Task icons. All are authored in a 20x20 viewport. -->
  <symbol id="task-user" viewBox="0 0 20 20" overflow="visible">
    <circle cx="10" cy="5" r="3" fill="none" stroke-width="1" />
    <path d="M 17,17 C 17,7 3,7 3,17 Z" fill="none" stroke="inherit" stroke-width="1" />
    <path fill="none" stroke="inherit" d="M 6,17 C 6,15 6,14 7,13" stroke-width="0.7" />
    <path fill="none" stroke="inherit" d="m 14,17 c 0,-2 0,-3 -1,-4" stroke-width="0.7" />
  </symbol>

  <symbol id="task-manual" viewBox="0 0 20 20" overflow="visible">
  <path style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 2,6 3,5 h 7 c 0.273837,1.0273837 0,1.3333333 -2,2 H 5 15 c 1,0 1,2 0,2 h -7 8 c 1,0 1,2 0,2 h -7 7 c 1,0 1,1.939827 0,1.939827 H 9 L 14,13 c 1,0 1,1.803733 0,1.803733 0,0 -7.00008,-0.02384 -9,0 C 3.6785895,14.819482 2,14 2,14" />
  </symbol>

  <symbol id="task-service" viewBox="0 0 20 20" overflow="visible">
    <g
       transform="translate(0.041016,0.43847664)">
      <path
         id="path23"
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 9.066406,2.0898437 C 8.785746,2.5974027 8.406338,3 8,3 7.595606,3 7.239756,2.6017747 6.980468,2.0976562 A 6,6 0 0 0 4.6191402,3.0761718 C 4.7917279,3.6153969 4.82095,4.1477996 4.5351559,4.4335937 4.2480065,4.720743 3.6972927,4.7048591 3.1406246,4.5449218 A 6,6 0 0 0 2.1386715,6.9375 c 0.5041758,0.259293 0.9023437,0.6170582 0.9023437,1.0214843 0,0.4063849 -0.4025156,0.7857368 -0.9101562,1.0664063 a 6,6 0 0 0 0.984375,2.4003904 c 0.2631149,-0.08457 0.5254974,-0.134733 0.7578125,-0.130859 0.2439954,0.0041 0.4551159,0.0684 0.6015625,0.214843 0.2871649,0.287166 0.2712897,0.839307 0.1113281,1.396485 A 6,6 0 0 0 6.980468,13.908203 C 7.239723,13.404452 7.595814,13.005859 8,13.005859 c 0.406132,0 0.785789,0.40101 1.066406,0.908203 a 6,6 0 0 0 2.435547,-1.007812 c -0.160414,-0.557144 -0.177774,-1.109336 0.109375,-1.396485 0.146446,-0.146446 0.357567,-0.210775 0.601562,-0.214843 0.232391,-0.0039 0.494342,0.04624 0.757813,0.130859 a 6,6 0 0 0 0.984375,-2.4003904 c -0.507136,-0.2806103 -0.908203,-0.660307 -0.908203,-1.0664063 0,-0.4041392 0.398675,-0.7602839 0.902343,-1.0195312 A 6,6 0 0 0 12.947265,4.5449218 C 12.390166,4.7048219 11.837907,4.7207192 11.550781,4.4335937 11.265003,4.1478157 11.293759,3.6153658 11.466797,3.0761718 A 6,6 0 0 0 9.066406,2.0898437 Z m -1.023438,2.4375 A 3.4752038,3.4752038 0 0 1 11.517578,8.0019531 3.4752038,3.4752038 0 0 1 8.042968,11.476562 3.4752038,3.4752038 0 0 1 4.568359,8.0019531 3.4752038,3.4752038 0 0 1 8.042968,4.5273437 Z" />
      <path
         id="path23-2"
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="m 12.898437,5.2089849 c -0.28066,0.507559 -0.660068,0.9101563 -1.066406,0.9101563 -0.404394,0 -0.760244,-0.3982253 -1.019532,-0.9023438 A 6,6 0 0 0 8.451171,6.195313 C 8.623759,6.7345381 8.652981,7.2669408 8.367191,7.5527349 8.080042,7.8398842 7.529328,7.8240003 6.97266,7.664063 a 6,6 0 0 0 -1.0019575,2.392578 c 0.5041755,0.259293 0.9023435,0.617058 0.9023435,1.021484 0,0.406385 -0.402515,0.785737 -0.910156,1.066407 a 6,6 0 0 0 0.984375,2.40039 c 0.263115,-0.08457 0.525497,-0.134733 0.757813,-0.130859 0.243995,0.0041 0.455115,0.0684 0.601562,0.214843 0.287165,0.287166 0.27129,0.839307 0.111328,1.396485 a 6,6 0 0 0 2.394531,1.001953 C 11.071754,16.523593 11.427845,16.125 11.832031,16.125 c 0.406132,0 0.785789,0.40101 1.066406,0.908203 a 6,6 0 0 0 2.435547,-1.007812 c -0.160414,-0.557144 -0.177774,-1.109336 0.109375,-1.396485 0.146446,-0.146446 0.357567,-0.210775 0.601562,-0.214843 0.232391,-0.0039 0.494342,0.04624 0.757813,0.130859 a 6,6 0 0 0 0.984375,-2.40039 c -0.507136,-0.280611 -0.908203,-0.660307 -0.908203,-1.066407 0,-0.404139 0.398675,-0.760283 0.902343,-1.019531 A 6,6 0 0 0 16.779296,7.664063 C 16.222197,7.8239631 15.669938,7.8398604 15.382812,7.5527349 15.097034,7.2669569 15.12579,6.734507 15.298828,6.195313 A 6,6 0 0 0 12.898437,5.2089849 Z m -1.023438,2.4375 a 3.4752038,3.4752038 0 0 1 3.47461,3.4746091 3.4752038,3.4752038 0 0 1 -3.47461,3.474609 3.4752038,3.4752038 0 0 1 -3.474609,-3.474609 3.4752038,3.4752038 0 0 1 3.474609,-3.4746091 z" />
    </g>
  </symbol>

  <symbol id="task-script" viewBox="0 0 20 20" overflow="visible">
      <path
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 6.1728012,4 H 13.694508"
         />
      <path
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 5.8271988,7 H 13.348908"
         />
      <path
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 7.8238944,13 H 15.3456"
          />
      <path
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 7.3054908,16 H 14.8272"
          />
      <path
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="m 6.807143,10 h 7.521711"
          />
  </symbol>

  <symbol id="task-business-rule" viewBox="0 0 20 20" overflow="visible">
      <rect
         style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         width="16"
         height="16"
         x="2"
         y="2" />
      <rect
         style="fill:#cccccc;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         width="16"
         height="4"
         x="2"
         y="2" />
      <path
         style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 2,10.000001 H 18" />
      <path
         style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 2,14.000004 H 18" />
      <path
         style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 6,6 V 18" />
  </symbol>

  <symbol id="task-send" viewBox="0 0 20 20" overflow="visible">
      <use href="#envelope-filled" x="0" y="0" width="20" height="20"/>
  </symbol>

  <symbol id="task-receive" viewBox="0 0 20 20" overflow="visible">
      <use href="#envelope-outline" x="0" y="0" width="20" height="20"/>
  </symbol>

  <!-- Event markers. All are authored in a 20x20 viewport. -->
  <symbol id="event-blank-catching" viewBox="0 0 20 20" overflow="visible">
    <!-- its blank, so there is nothing in here -->
  </symbol>


  <symbol id="event-message-catching" viewBox="0 0 20 20" overflow="visible">
    <use href="#envelope-outline" x="0" y="0" width="20" height="20"/>
  </symbol>

  <symbol id="event-message-throwing" viewBox="0 0 20 20" overflow="visible">
    <use href="#envelope-filled" x="0" y="0" width="20" height="20"/>
  </symbol>

  <symbol id="event-timer-catching" viewBox="0 0 20 20" overflow="visible">
      <g
         transform="matrix(1.1171135,0,0,1.1171135,-1.1711354,-1.1711354)">
        <circle
           style="fill:none;stroke:inherit;stroke-width:0.71613135;stroke-linecap:round;stroke-linejoin:round"
           cx="10"
           cy="10"
           r="6.6344123" />
        <g
           style="stroke-width:0.6"
           transform="matrix(0.82930155,0,0,0.82930155,1.7069845,1.7069845)">
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 10,2 V 4"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="m 10,16 v 2"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 14,3.0717968 13,4.8038476"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 7,15.196152 6,16.928203"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 16.928203,6 15.196152,7"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 4.8038476,13 3.0717968,14"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 18,10 H 16"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 4,10 H 2"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 16.928203,14 15.196152,13"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 4.8038476,7 3.0717968,6"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 14,16.928203 13,15.196152"
             />
          <path
             style="fill:none;stroke:inherit;stroke-width:0.5397097;stroke-linecap:round;stroke-linejoin:round"
             d="M 7,4.8038476 6,3.0717968"
             />
        </g>
        <path
           style="fill:none;stroke:inherit;stroke-width:0.71613135;stroke-linecap:round;stroke-linejoin:round"
           d="M 11.509767,4.9497724 10,10 l 3.317206,0.829302" />
      </g>
  </symbol>

  <symbol id="event-error-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 4,14 8,4 12,11 16,6 12,16 8,9 Z" />
  </symbol>

  <symbol id="event-error-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 4,14 8,4 12,11 16,6 12,16 8,9 Z" />
  </symbol>

  <symbol id="event-escalation-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 10,3 -4,13 4,-5 4,5 z" />
  </symbol>

  <symbol id="event-escalation-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 10,3 -4,13 4,-5 4,5 z" />
  </symbol>

  <symbol id="event-cancel-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 4,6 2,-2 4,4 4,-4 2,2 -4,4 4,4 -2,2 -4,-4 -4,4 -2,-2 4,-4 z"
     />
  </symbol>

  <symbol id="event-cancel-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round"
     d="m 4,6 2,-2 4,4 4,-4 2,2 -4,4 4,4 -2,2 -4,-4 -4,4 -2,-2 4,-4 z"
     />
  </symbol>

  <symbol id="event-compensation-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 4.0513388,10 5,-5 v 10 z" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 9.5892896,10 14.589288,5 v 10 z" />
  </symbol>

  <symbol id="event-compensation-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 4.0513388,10 5,-5 v 10 z" />
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 9.5892896,10 14.589288,5 v 10 z" />
  </symbol>

  <symbol id="event-conditional-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 6,4 h 8 V 16 H 6 Z" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 7,6 h 6" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 7,8 h 6" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 13,10 H 7" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 7,12 h 6" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 13,14 H 7" />
  </symbol>

  <symbol id="event-link-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 4,7 h 8 V 3 l 5,7 -5,7 V 13 H 4 Z" />
  </symbol>

  <symbol id="event-link-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 4,7 h 8 V 3 l 5,7 -5,7 V 13 H 4 Z" />
  </symbol>

  <symbol id="event-signal-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 10,3 4,14 h 12 z" />
  </symbol>

  <symbol id="event-signal-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 10,3 4,14 h 12 z" />
  </symbol>

  <symbol id="event-termination-throwing" viewBox="0 0 20 20" overflow="visible">
  <circle
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     cx="10"
     cy="10"
     r="7.0710678" />
  </symbol>

  <symbol id="event-multiple-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 10,3 -7,5 3,8 h 8 l 3,-8 z" />
  </symbol>

  <symbol id="event-multiple-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 10,3 -7,5 3,8 h 8 l 3,-8 z" />
  </symbol>

  <symbol id="event-multiple-parallel-catching" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 8.5857864,2.9289322 h 2.8284276 v 5.6568542 l 5.656854,0 v 2.8284276 h -5.656854 l 0,5.656854 H 8.5857864 V 11.414214 H 2.9289322 V 8.5857864 h 5.6568542 z" />
  </symbol>

  <symbol id="event-multiple-parallel-throwing" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="m 8.5857864,2.9289322 h 2.8284276 v 5.6568542 l 5.656854,0 v 2.8284276 h -5.656854 l 0,5.656854 H 8.5857864 V 11.414214 H 2.9289322 V 8.5857864 h 5.6568542 z" />
  </symbol>

  <symbol id="event-solid" viewBox="0 0 20 20" overflow="visible">
  <circle
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     cx="10" cy="10" r="10" />
  </symbol>

  <symbol id="event-solid-solid" viewBox="0 0 20 20" overflow="visible">
  <circle
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     cx="10" cy="10" r="10" />
  <circle
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     cx="10" cy="10" r="8.6939783" />
  </symbol>

  <symbol id="event-dashed" viewBox="0 0 20 20" overflow="visible">
  <circle
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:2.40000006,2.40000006;stroke-dashoffset:0"
     cx="10"
     cy="10"
     r="10" />
  </symbol>

  <symbol id="event-dashed-dashed" viewBox="0 0 20 20" overflow="visible">
  <circle
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:2.40000006,2.40000006;stroke-dashoffset:0"
     cx="10"
     cy="10"
     r="10" />
  <path
     style="baseline-shift:baseline;display:inline;overflow:visible;fill:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none;enable-background:accumulate;stop-color:inherit;stop-opacity:1;opacity:1;stroke:inherit;stroke-width:0.11"
     d="M 10.787109,0.99609375 A 0.347752,0.347752 0 0 0 10.410156,1.3105469 0.347752,0.347752 0 0 0 10.724609,1.6875 l 0.128907,0.011719 0.210937,0.023437 0.207031,0.029297 0.207032,0.035156 0.203125,0.039063 0.203125,0.044922 0.201172,0.048828 0.199218,0.052734 0.197266,0.058594 0.195312,0.064453 a 0.347752,0.347752 0 0 0 0.44336,-0.2128906 0.347752,0.347752 0 0 0 -0.212891,-0.4433594 l -0.0039,-0.00195 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00195 l -0.203125,-0.066406 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00195 l -0.207031,-0.0625 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00195 l -0.207031,-0.056641 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00195 l -0.208984,-0.050781 a 0.34778678,0.34778678 0 0 0 -0.0098,-0.00195 l -0.210937,-0.044922 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00195 l -0.21289,-0.041016 a 0.34778678,0.34778678 0 0 0 -0.0098,-0.00195 l -0.214844,-0.035156 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00195 l -0.216797,-0.03125 a 0.34778678,0.34778678 0 0 0 -0.0098,0 l -0.21875,-0.025391 a 0.34778678,0.34778678 0 0 0 -0.0078,0 z M 8.6191406,1.0664062 8.4042969,1.1015625 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00195 l -0.2128906,0.041016 a 0.34778678,0.34778678 0 0 0 -0.00977,0.00195 l -0.2109375,0.044922 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00195 l -0.2109375,0.050781 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00195 l -0.2070312,0.056641 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00195 l -0.2050782,0.0625 a 0.34778678,0.34778678 0 0 0 -0.00977,0.00195 l -0.203125,0.066406 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00195 l -0.2011719,0.070312 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00391 l -0.1972656,0.074219 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00391 l -0.1425781,0.058594 a 0.347752,0.347752 0 0 0 -0.1894531,0.453125 0.347752,0.347752 0 0 0 0.453125,0.1894531 L 6.9394531,2.234375 7.1308594,2.1621094 7.3242187,2.0957031 7.5175781,2.03125 7.7148437,1.9726562 7.9140625,1.9199219 8.1152344,1.8710937 8.3183594,1.8261719 8.5214844,1.7871094 8.7324219,1.7519531 A 0.347752,0.347752 0 0 0 9.0195313,1.3515625 0.347752,0.347752 0 0 0 8.6191406,1.0664062 Z M 14.853516,2.3730469 A 0.347752,0.347752 0 0 0 14.375,2.484375 0.347752,0.347752 0 0 0 14.486328,2.9628906 l 0.01172,0.00781 0.167969,0.1113282 0.166015,0.1132812 0.16211,0.1191406 0.158203,0.1210938 0.15625,0.1269531 0.154297,0.1289063 0.148437,0.1328125 0.146485,0.1367187 0.144531,0.140625 0.126953,0.1308594 a 0.347752,0.347752 0 0 0 0.492187,0.00586 0.347752,0.347752 0 0 0 0.0059,-0.4921874 l -0.13086,-0.1347657 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00586 L 16.242187,3.4609375 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00586 L 16.082031,3.3125 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00586 l -0.15625,-0.1386718 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00391 L 15.753906,3.0292969 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00586 L 15.585938,2.8925781 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00586 L 15.414063,2.7597656 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00586 L 15.238281,2.6308594 a 0.34778678,0.34778678 0 0 0 -0.0078,-0.00391 L 15.058594,2.5078125 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00391 L 14.876953,2.3886719 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00586 z M 4.6542969,2.7109375 4.59375,2.7539063 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00586 L 4.421875,2.8867188 a 0.34778678,0.34778678 0 0 0 -0.00781,0.00586 L 4.2519531,3.0234375 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00586 L 4.0859375,3.1640625 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00391 l -0.15625,0.1386718 A 0.34778678,0.34778678 0 0 0 3.9179688,3.3125 L 3.765625,3.4550781 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00586 L 3.609375,3.6054688 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00586 L 3.4589844,3.7617188 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00586 L 3.3105469,3.9199219 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00586 L 3.1660156,4.0820312 a 0.34778678,0.34778678 0 0 0 -0.00586,0.00586 L 3.0742188,4.1914062 A 0.347752,0.347752 0 0 0 3.1152344,4.6816406 0.347752,0.347752 0 0 0 3.6054688,4.640625 L 3.6894531,4.5410156 3.8222656,4.390625 3.9589844,4.2441406 4.0996094,4.1015625 4.2421875,3.9609375 4.3886719,3.8242188 4.5390625,3.6914063 4.6914062,3.5625 4.8476562,3.4355469 5.0058594,3.3144531 5.0625,3.2734375 A 0.347752,0.347752 0 0 0 5.1386719,2.7871094 0.347752,0.347752 0 0 0 4.6542969,2.7109375 Z M 17.351562,5.3535156 a 0.347752,0.347752 0 0 0 -0.121093,0.4765625 l 0.0098,0.017578 0.09961,0.1777344 0.09375,0.1777344 0.08984,0.1816406 0.08594,0.1835938 0.08203,0.1855468 0.07617,0.1875 0.07227,0.1914063 0.06836,0.1933594 0.0625,0.1933593 0.04297,0.1464844 a 0.347752,0.347752 0 0 0 0.433594,0.234375 0.347752,0.347752 0 0 0 0.234375,-0.4316406 l -0.04492,-0.1503906 a 0.34778678,0.34778678 0 0 0 -0.002,-0.00781 l -0.06641,-0.203125 a 0.34778678,0.34778678 0 0 0 -0.0039,-0.00977 l -0.07031,-0.1992187 a 0.34778678,0.34778678 0 0 0 -0.002,-0.00781 l -0.07617,-0.1992188 a 0.34778678,0.34778678 0 0 0 -0.0039,-0.00781 l -0.08008,-0.1972656 a 0.34778678,0.34778678 0 0 0 -0.002,-0.00781 l -0.08594,-0.1933594 a 0.34778678,0.34778678 0 0 0 -0.0039,-0.00781 l -0.08789,-0.1914062 a 0.34778678,0.34778678 0 0 0 -0.0039,-0.00781 l -0.09375,-0.1875 a 0.34778678,0.34778678 0 0 0 -0.0039,-0.00781 l -0.09766,-0.1875 a 0.34778678,0.34778678 0 0 0 -0.0059,-0.00586 L 17.845703,5.5058594 a 0.34778678,0.34778678 0 0 0 -0.0039,-0.00781 l -0.01367,-0.021484 A 0.347752,0.347752 0 0 0 17.351562,5.3535156 Z M 2.3554688,5.8457031 A 0.347752,0.347752 0 0 0 1.890625,6.0039062 l -0.039063,0.074219 a 0.34778678,0.34778678 0 0 0 -0.00391,0.00781 l -0.087891,0.1914062 a 0.34778678,0.34778678 0 0 0 -0.00391,0.00781 L 1.671875,6.4785156 a 0.34778678,0.34778678 0 0 0 -0.00391,0.00781 l -0.080078,0.1972656 a 0.34778678,0.34778678 0 0 0 -0.00391,0.00781 L 1.5097656,6.890625 a 0.34778678,0.34778678 0 0 0 -0.00391,0.00781 l -0.070312,0.1992187 a 0.34778678,0.34778678 0 0 0 -0.00391,0.00977 l -0.064453,0.203125 a 0.34778678,0.34778678 0 0 0 -0.00391,0.00781 l -0.060547,0.2050781 a 0.34778678,0.34778678 0 0 0 -0.00195,0.00781 l -0.056641,0.2070312 a 0.34778678,0.34778678 0 0 0 -0.00195,0.00977 l -0.050781,0.2089843 a 0.34778678,0.34778678 0 0 0 -0.00195,0.00781 L 1.171875,8.046875 A 0.347752,0.347752 0 0 0 1.4375,8.4609375 0.347752,0.347752 0 0 0 1.8515625,8.1953125 l 0.015625,-0.078125 0.048828,-0.2011719 0.054687,-0.1992187 0.058594,-0.1972657 0.0625,-0.1933593 0.068359,-0.1933594 0.072266,-0.1914063 0.078125,-0.1875 L 2.390625,6.5683594 2.4765625,6.3847656 2.5117188,6.3125 A 0.347752,0.347752 0 0 0 2.3554688,5.8457031 Z M 18.671875,9.2734375 a 0.347752,0.347752 0 0 0 -0.333984,0.3613281 l 0.0059,0.1523438 0.002,0.2109375 v 0.00977 l -0.002,0.210937 -0.0078,0.212891 -0.01367,0.21289 -0.01953,0.210938 -0.02344,0.210937 -0.0293,0.207032 -0.0332,0.207031 -0.04102,0.205078 -0.04297,0.201172 -0.02539,0.09961 a 0.347752,0.347752 0 0 0 0.255859,0.419922 0.347752,0.347752 0 0 0 0.419922,-0.253906 l 0.02539,-0.103516 a 0.34778678,0.34778678 0 0 0 0.002,-0.0098 l 0.04687,-0.210938 a 0.34778678,0.34778678 0 0 0 0.002,-0.0078 l 0.04102,-0.212891 a 0.34778678,0.34778678 0 0 0 0,-0.0098 l 0.03711,-0.214843 a 0.34778678,0.34778678 0 0 0 0,-0.0078 l 0.03125,-0.216797 a 0.34778678,0.34778678 0 0 0 0,-0.0098 l 0.02539,-0.216797 a 0.34778678,0.34778678 0 0 0 0.002,-0.0098 l 0.01953,-0.21875 a 0.34778678,0.34778678 0 0 0 0,-0.0098 l 0.01367,-0.220703 a 0.34778678,0.34778678 0 0 0 0,-0.0098 l 0.0098,-0.222657 a 0.34778678,0.34778678 0 0 0 0,-0.0098 l 0.002,-0.222656 a 0.347752,0.347752 0 0 0 -0.0039,-0.0059 0.347752,0.347752 0 0 0 0.0039,-0.00391 l -0.002,-0.2246094 a 0.34778678,0.34778678 0 0 0 0,-0.00781 l -0.0059,-0.1582031 A 0.347752,0.347752 0 0 0 18.671875,9.2734375 Z M 1.3046875,9.8457031 a 0.347752,0.347752 0 0 0 -0.34375,0.3515629 v 0.0332 a 0.34778678,0.34778678 0 0 0 0,0.0098 l 0.009766,0.222657 a 0.34778678,0.34778678 0 0 0 0,0.0098 l 0.0136719,0.220703 a 0.34778678,0.34778678 0 0 0 0,0.0098 l 0.0195312,0.21875 a 0.34778678,0.34778678 0 0 0 0.00195,0.0098 l 0.025391,0.216797 a 0.34778678,0.34778678 0 0 0 0,0.0098 L 1.0625,11.375 a 0.34778678,0.34778678 0 0 0 0.00195,0.0078 l 0.035156,0.214843 a 0.34778678,0.34778678 0 0 0 0.00195,0.0098 l 0.039063,0.212891 a 0.34778678,0.34778678 0 0 0 0.00195,0.0078 l 0.046875,0.210938 a 0.34778678,0.34778678 0 0 0 0.00195,0.0098 l 0.050781,0.208985 a 0.34778678,0.34778678 0 0 0 0.00195,0.0078 l 0.021484,0.08008 A 0.347752,0.347752 0 0 0 1.6914062,12.589844 0.347752,0.347752 0 0 0 1.9375,12.164063 L 1.9160156,12.087891 1.8671875,11.886719 1.8242187,11.685547 1.7851562,11.480469 1.75,11.273438 1.7207031,11.066406 1.6972656,10.855469 1.6777344,10.644531 1.6640625,10.431641 1.65625,10.21875 v -0.0293 A 0.347752,0.347752 0 0 0 1.3046875,9.8457031 Z M 17.875,13.708984 a 0.347752,0.347752 0 0 0 -0.46875,0.146485 l -0.06641,0.125 -0.09961,0.175781 -0.101562,0.173828 L 17.03125,14.5 l -0.109375,0.167969 -0.115234,0.166015 -0.119141,0.16211 -0.121094,0.158203 -0.125,0.15625 -0.130859,0.154297 -0.03516,0.03906 a 0.347752,0.347752 0 0 0 0.03125,0.490235 0.347752,0.347752 0 0 0 0.490234,-0.0293 l 0.03711,-0.04297 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0059 l 0.134765,-0.158203 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0078 l 0.130859,-0.162109 a 0.34778678,0.34778678 0 0 0 0.0039,-0.0059 l 0.126953,-0.166015 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0078 l 0.123047,-0.167969 a 0.34778678,0.34778678 0 0 0 0.0039,-0.0078 l 0.119141,-0.171875 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0059 l 0.115234,-0.175782 a 0.34778678,0.34778678 0 0 0 0.0039,-0.0059 l 0.111328,-0.177734 a 0.34778678,0.34778678 0 0 0 0.0039,-0.0078 l 0.107422,-0.179687 a 0.34778678,0.34778678 0 0 0 0.0039,-0.0078 l 0.101563,-0.183594 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0078 l 0.06641,-0.128906 A 0.347752,0.347752 0 0 0 17.875,13.708984 Z M 2.2070313,13.882813 A 0.347752,0.347752 0 0 0 2.0742188,14.355469 L 2.1542969,14.5 a 0.34778678,0.34778678 0 0 0 0.00391,0.0078 L 2.265625,14.6875 a 0.34778678,0.34778678 0 0 0 0.00391,0.0078 l 0.1113281,0.177734 a 0.34778678,0.34778678 0 0 0 0.00391,0.0059 L 2.5,15.054688 a 0.34778678,0.34778678 0 0 0 0.00586,0.0059 L 2.625,15.232422 a 0.34778678,0.34778678 0 0 0 0.00391,0.0078 l 0.1230468,0.167969 a 0.34778678,0.34778678 0 0 0 0.00586,0.0078 l 0.1269531,0.166015 a 0.34778678,0.34778678 0 0 0 0.00586,0.0059 L 3.0214844,15.75 a 0.34778678,0.34778678 0 0 0 0.00391,0.0078 l 0.1347657,0.158203 a 0.34778678,0.34778678 0 0 0 0.00586,0.0059 l 0.1386719,0.15625 a 0.34778678,0.34778678 0 0 0 0.00586,0.0059 l 0.025391,0.0293 a 0.347752,0.347752 0 0 0 0.4921875,0.01758 0.347752,0.347752 0 0 0 0.017578,-0.492187 L 3.8222656,15.613281 3.6894531,15.464844 3.5605469,15.310547 3.4335938,15.154297 3.3125,14.996094 3.1933594,14.833984 3.0800781,14.667969 2.96875,14.5 2.8613281,14.330078 2.7597656,14.15625 2.6816406,14.015625 A 0.347752,0.347752 0 0 0 2.2070313,13.882813 Z m 12.5820317,2.955078 -0.123047,0.08594 -0.167969,0.109375 -0.169922,0.107422 -0.173828,0.101562 -0.175781,0.09961 -0.179688,0.09375 -0.179687,0.08984 -0.183594,0.08594 -0.1875,0.08203 -0.1875,0.07617 -0.03125,0.01172 a 0.347752,0.347752 0 0 0 -0.201172,0.449219 0.347752,0.347752 0 0 0 0.447266,0.201172 l 0.03711,-0.01367 a 0.34778678,0.34778678 0 0 0 0.0078,-0.002 l 0.195312,-0.08008 a 0.34778678,0.34778678 0 0 0 0.0078,-0.0039 l 0.193359,-0.08594 a 0.34778678,0.34778678 0 0 0 0.0078,-0.002 l 0.191407,-0.08984 a 0.34778678,0.34778678 0 0 0 0.0078,-0.0039 l 0.189453,-0.09375 a 0.34778678,0.34778678 0 0 0 0.0078,-0.0039 l 0.185547,-0.09766 a 0.34778678,0.34778678 0 0 0 0.0078,-0.0039 l 0.183594,-0.103516 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0039 l 0.181641,-0.107422 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0039 l 0.179688,-0.111328 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0039 l 0.175781,-0.115234 a 0.34778678,0.34778678 0 0 0 0.0059,-0.0039 l 0.126953,-0.08789 a 0.347752,0.347752 0 0 0 0.08789,-0.484375 0.347752,0.347752 0 0 0 -0.484375,-0.08789 z m -9.4277349,0.103515 a 0.347752,0.347752 0 0 0 -0.4804687,0.09961 0.347752,0.347752 0 0 0 0.099609,0.482421 l 0.1425782,0.09375 a 0.34778678,0.34778678 0 0 0 0.00781,0.0039 l 0.1777343,0.111328 a 0.34778678,0.34778678 0 0 0 0.00586,0.0039 l 0.1816406,0.107422 a 0.34778678,0.34778678 0 0 0 0.00781,0.0039 l 0.1816407,0.103516 a 0.34778678,0.34778678 0 0 0 0.00781,0.0039 l 0.1875,0.09766 a 0.34778678,0.34778678 0 0 0 0.00586,0.0039 l 0.1894532,0.09375 a 0.34778678,0.34778678 0 0 0 0.00781,0.0039 l 0.1914062,0.08984 a 0.34778678,0.34778678 0 0 0 0.00781,0.002 l 0.1933594,0.08594 a 0.34778678,0.34778678 0 0 0 0.00781,0.0039 l 0.1972656,0.08008 a 0.34778678,0.34778678 0 0 0 0.00781,0.002 l 0.1972656,0.07617 a 0.34778678,0.34778678 0 0 0 0.00781,0.002 l 0.017578,0.0059 A 0.347752,0.347752 0 0 0 7.3554687,18.289062 0.347752,0.347752 0 0 0 7.1425781,17.845703 l -0.011719,-0.0039 -0.1914063,-0.07227 -0.1875,-0.07617 L 6.5664062,17.611328 6.3828125,17.525391 6.2011719,17.435547 6.0214844,17.341797 5.8457031,17.242187 5.671875,17.140625 5.5019531,17.033203 Z m 5.7265629,1.335938 -0.02344,0.0039 -0.210937,0.02344 -0.210938,0.01953 -0.21289,0.01367 -0.214844,0.0078 -0.214844,0.002 -0.2148437,-0.002 -0.2148438,-0.0078 -0.2128906,-0.01367 -0.2109375,-0.01953 -0.052734,-0.0059 a 0.347752,0.347752 0 0 0 -0.3847656,0.306641 0.347752,0.347752 0 0 0 0.3066406,0.384765 l 0.056641,0.0059 a 0.34778678,0.34778678 0 0 0 0.00781,0.002 l 0.2207032,0.01953 a 0.34778678,0.34778678 0 0 0 0.00781,0 L 9.53125,19.03125 a 0.34778678,0.34778678 0 0 0 0.00781,0.002 l 0.2226563,0.0078 a 0.34778678,0.34778678 0 0 0 0.00977,0 l 0.2246094,0.0039 a 0.34778678,0.34778678 0 0 0 0.00781,0 l 0.22461,-0.0039 a 0.34778678,0.34778678 0 0 0 0.0098,0 l 0.222657,-0.0078 a 0.34778678,0.34778678 0 0 0 0.0078,-0.002 l 0.222656,-0.01367 a 0.34778678,0.34778678 0 0 0 0.0078,0 l 0.220703,-0.01953 a 0.34778678,0.34778678 0 0 0 0.0078,-0.002 l 0.21875,-0.02344 a 0.34778678,0.34778678 0 0 0 0.0098,-0.002 l 0.02734,-0.0039 a 0.347752,0.347752 0 0 0 0.296875,-0.392578 0.347752,0.347752 0 0 0 -0.392578,-0.296875 z" />
  </symbol>

  <symbol id="event-thick" viewBox="0 0 20 20" overflow="visible">
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none;stroke-dashoffset:0"
     d="M 10 0 A 10 10 0 0 0 0 10 A 10 10 0 0 0 10 20 A 10 10 0 0 0 20 10 A 10 10 0 0 0 10 0 z M 10 1.3066406 A 8.6939783 8.6939783 0 0 1 18.693359 10 A 8.6939783 8.6939783 0 0 1 10 18.693359 A 8.6939783 8.6939783 0 0 1 1.3066406 10 A 8.6939783 8.6939783 0 0 1 10 1.3066406 z " />
  </symbol>

  <symbol id="task-box" viewBox="0 0 {ACTIVITY_NODE_WIDTH} {ACTIVITY_NODE_HEIGHT}" overflow="visible" >
  <rect
     width="{ACTIVITY_NODE_WIDTH}"
     height="{ACTIVITY_NODE_HEIGHT}"
     x="0"
     y="0"
     ry="5" />
  </symbol>

  <symbol id="gateway-box" viewBox="0 0 {GATEWAY_NODE_WIDTH} {GATEWAY_NODE_HEIGHT}" overflow="visible" >
  <path stroke="inherit" d="M25 0 L50 25 L25 50 L0 25 z" />
  </symbol>

  <!-- Gateway markers. -->

  <symbol id="gateway-exclusive" viewBox="0 0 50 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none;stroke-dashoffset:0;stroke-opacity:1"
     d="M 25,0 0,25 25,50 50,25 Z" />
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none;stroke-dashoffset:0;stroke-opacity:1"
     d="m 15,12 h 4 l 6,11 6,-11 h 4 l -8,13 8,13 H 31 L 25,27 19,38 h -4 l 8,-13 z" />
  </symbol>

  <symbol id="gateway-inclusive" viewBox="0 0 50 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none;stroke-dashoffset:0;stroke-opacity:1"
     d="M 25,0 0,25 25,50 50,25 Z" />
  <circle
     style="fill:none;stroke:inherit;stroke-width:3.3;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none;stroke-dashoffset:0;stroke-opacity:1"
     cx="25"
     cy="25"
     r="13.12418" />
  </symbol>

  <symbol id="gateway-parallel" viewBox="0 0 50 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 25,0 0,25 25,50 50,25 Z" />
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="m 23,9 h 4 v 14 h 14 v 4 H 27 V 41 H 23 V 27 H 9 v -4 h 14 z" />
  </symbol>

  <symbol id="gateway-complex" viewBox="0 0 50 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 25,0 0,25 25,50 50,25 Z" />
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="m 23,10 h 4 v 13 h 13 v 4 H 27 V 40 H 23 V 27 H 10 v -4 h 13 z" />
  <path
     style="fill:inherit;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 34.192388,12.979185 37.020815,15.807612 27.828427,25 37.020815,34.192388 34.192388,37.020815 25,27.828427 15.807612,37.020815 12.979185,34.192388 22.171573,25 12.979185,15.807612 15.807612,12.979185 25,22.171573 Z" />
  </symbol>

  <symbol id="gateway-event" viewBox="0 0 50 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 25,0 0,25 25,50 50,25 Z" />
  <circle
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     cx="25"
     cy="25"
     r="15" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="m 25,14 -10,9 5,11 h 10 l 5,-11 z" />
  </symbol>

  <symbol id="gateway-parallel-event" viewBox="0 0 50 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 25,0 0,25 25,50 50,25 Z" />
  <circle
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     cx="25"
     cy="25"
     r="15" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="m 23,15 h 4 v 8 h 8 v 4 h -8 v 8 h -4 v -8 h -8 v -4 h 8 z" />
  </symbol>


  <symbol id="three-stripes" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 0.4,0.40000001 V 6.4" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 1.9,0.40000001 V 6.4" />
  <path
     style="fill:none;stroke:inherit;stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 3.4,0.40000001 V 6.4" />
  </symbol>

  <!-- Data shapes authored in a 60x70 viewport. -->
  <symbol id="data-object" viewBox="0 0 36 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 0,0 V 50 H 36 V 12 L 23,0 Z" />
  <path
     style="fill:none;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 23,0 V 12 H 36" />
  </symbol>

  <symbol id="data-input" viewBox="0 0 36 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 0,0 V 50 H 36 V 12 L 23,0 Z" />
  <path
     style="fill:none;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 23,0 V 12 H 36" />
  <path
     style="fill:none;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="m 3,8 h 9 V 3 l 7,8 -7,8 V 14 H 3 Z" />
  </symbol>

  <symbol id="data-output" viewBox="0 0 36 50" overflow="visible">
  <path
     style="fill:none;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 0,0 V 50 H 36 V 12 L 23,0 Z" />
  <path
     style="fill:none;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="M 23,0 V 12 H 36" />
  <path
     style="fill:inherit;stroke:inherit;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="m 3,8 h 9 V 3 l 7,8 -7,8 V 14 H 3 Z" />
  </symbol>

  <symbol id="data-store" viewBox="0 0 50 50" overflow="visible">
  <path
     style="fill:none;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
     d="m 0,5 v 40 c 0,2.761424 11.192881,5 25,5 13.807119,0 25,-2.238576 25,-5 V 5" />
  <ellipse
     style="fill:none;stroke:inherit;stroke-linecap:square;stroke-linejoin:miter;stroke-dasharray:none"
     cx="25"
     cy="5"
     rx="25"
     ry="5" />
  <path
     style="fill:none;stroke:inherit;stroke-linecap:square"
     d="M 50,7 C 50,9.7614237 38.807119,12 25,12 11.192881,12 0,9.7614237 0,7" />
  <path
     style="fill:none;stroke:inherit;stroke-linecap:square"
     d="M 50,9 C 50,11.761424 38.807119,14 25,14 11.192881,14 0,11.761424 0,9" />
  </symbol>
</defs>"##,
    )
}
