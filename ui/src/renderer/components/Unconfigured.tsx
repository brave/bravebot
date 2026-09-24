import { Modal } from './Modal'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './ui/accordion'
import { Button } from './ui/button'
import { DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'
export function Unconfigured({ detail, onClose }: { detail: string; onClose: () => void }): React.JSX.Element {
  return <Modal title="Backend setup" onClose={onClose}>
    <DialogHeader>
      <DialogTitle>Connect the agent backend</DialogTitle>
      <DialogDescription>This build cannot load its backend credentials. You can continue browsing conversations and writing drafts.</DialogDescription>
    </DialogHeader>
    <p>If you installed Brave Bot, obtain a configured build from its distributor. Credentials are currently provided when the agent is built.</p>
    <Accordion type="multiple">
      <AccordionItem value="development">
        <AccordionTrigger>Development setup</AccordionTrigger>
        <AccordionContent><ol>
          <li>Provide the backend credentials through the project’s approved environment configuration.</li>
          <li>Run <code>npm run bridge</code> from the interface checkout.</li>
          <li>Restart Brave Bot, then use <strong>Check again</strong>.</li>
        </ol><p>See <code>docs/setup.md</code> for the configuration layout. Keep credentials outside the repository.</p></AccordionContent>
      </AccordionItem>
      <AccordionItem value="technical">
        <AccordionTrigger>Technical details</AccordionTrigger>
        <AccordionContent><pre>{detail}</pre></AccordionContent>
      </AccordionItem>
    </Accordion>
    <DialogFooter><Button onClick={onClose}>Continue browsing</Button></DialogFooter>
  </Modal>
}
